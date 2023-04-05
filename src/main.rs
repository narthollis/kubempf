use anyhow::Context;
use futures::{StreamExt, TryStreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_stream::wrappers::TcpListenerStream;
use tracing::*;

use k8s_openapi::api::core::v1::Service;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::util::intstr::IntOrString};
use kube::{
    api::{Api, ListParams},
    Client, Config,
};

pub(crate) mod errors;
pub(crate) mod cli;
use crate::errors::MyError;
use crate::cli::{cli, Forward};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = cli().get_matches();

    let context = matches.get_one::<String>("context");
    let namespace = matches.get_one::<String>("namespace");
    // this unwrap should not error as cli().get_matches() should return help if no forwards are provided
    let forwards = matches.get_many::<String>("forwards").unwrap();

    tracing_subscriber::fmt::init();

    let kube_opts = kube::config::KubeConfigOptions {
        context: context.map(|s| (*s).clone()),
        cluster: None,
        user: None,
    };
    let mut config = Config::from_kubeconfig(&kube_opts).await?;
    if let Some(ns) = namespace {
        config.default_namespace = (*ns).clone();
    }

    let client = Client::try_from(config)?;

    let service_api: Api<Service> = Api::default_namespaced(client.clone());

    let mut handles = Vec::<JoinHandle<anyhow::Result<()>>>::with_capacity(forwards.len());

    for (i, arg) in forwards.enumerate() {
        let forward = Forward::parse(&service_api, arg).await?;

        info!(
            "Forwarding {} to {}:{}",
            forward.local_address, forward.service_name, forward.service_port
        );

        let socket = TcpListener::bind(forward.local_address).await?;

        handles.insert(
            i,
            tokio::spawn(serve(
                socket,
                client.clone(),
                forward.pod_list_params,
                forward.pod_port,
            )),
        );
    }

    info!("Ctrl-C to stop the server");
    futures::future::join_all(handles).await;

    Ok(())
}

async fn serve(
    socket: TcpListener,
    client: Client,
    selector: ListParams,
    pod_port: IntOrString,
    ) -> anyhow::Result<()> {
    TcpListenerStream::new(socket)
        .take_until(tokio::signal::ctrl_c())
        .try_for_each(|client_conn| async {
            if let Ok(peer_addr) = client_conn.peer_addr() {
                info!(%peer_addr, "new connection");
            }

            let pod_api: Api<Pod> = Api::default_namespaced(client.clone());
            let sel = selector.clone();
            let port = pod_port.clone();

            tokio::spawn(async move {
                if let Err(e) = forward_connection(&pod_api, &sel, &port, client_conn).await {
                    error!(
                        error = e.as_ref() as &dyn std::error::Error,
                        "failed to forward connection"
                    );
                }
            });

            Ok(())
        })
        .await?;
    info!("done");
    Ok(())
}

const EMPTY_POD_LIST: kube::core::ObjectList<Pod> = kube::core::ObjectList::<Pod> {
    metadata: kube::core::ListMeta {
        continue_: None,
        remaining_item_count: None,
        resource_version: None,
        self_link: None,
    },
    items: vec![],
};

async fn find_pod(api: &Api<Pod>, selector: &ListParams) -> anyhow::Result<Pod> {
    api.list(selector)
        .await
        .unwrap_or(EMPTY_POD_LIST)
        .items
        .into_iter()
        .find(|p| {
            p.status.as_ref().map_or(false, |s| {
                s.conditions.as_ref().map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                })
            })
        })
        .ok_or(MyError::MatchingReadyPodNotFound().into())
}

fn port_to_int(pod_port: &IntOrString, pod: &Pod) -> anyhow::Result<u16> {
    match pod_port.clone() {
        IntOrString::Int(i) => {
            u16::try_from(i).map_err(|_| MyError::CouldNotFindPort(pod_port.clone()).into())
        }
        IntOrString::String(n) => pod
            .spec
            .clone()
            .and_then(|s| {
                s.containers
                    .into_iter()
                    .flat_map(|c| c.ports.unwrap_or(vec![]))
                    .find(|p| p.name == Some(n.clone()))
            })
            .and_then(|p| u16::try_from(p.container_port).ok())
            .ok_or(MyError::CouldNotFindPort(pod_port.clone()).into()),
    }
}

async fn forward_connection(
    pod_api: &Api<Pod>,
    selector: &ListParams,
    pod_port: &IntOrString,
    mut client_conn: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let pod = find_pod(pod_api, selector).await?;
    let port = port_to_int(pod_port, &pod)?;
    let pod_name = pod.metadata.name.unwrap(); // how on earth you would end up here without a pod name is beyond me

    let mut forwarder = pod_api.portforward(pod_name.as_str(), &[port]).await?;
    let mut upstream_conn = forwarder
        .take_stream(port)
        .context("port not found in forwarder")?;
    tokio::io::copy_bidirectional(&mut client_conn, &mut upstream_conn).await?;
    drop(upstream_conn);
    forwarder.join().await?;
    info!("connection closed");
    Ok(())
}
