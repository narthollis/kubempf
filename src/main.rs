use anyhow::Context;
use futures::{StreamExt, TryStreamExt};
use std::{collections::BTreeMap, env, net::SocketAddr};
use thiserror::Error;
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
    Client,
};
use clap::{arg, Command};

fn cli() -> Command {
    Command::new("kubempf")
        .about("Multi-service port proxying tool for Kubernetes")
        .arg_required_else_help(true)
        .allow_external_subcommands(false)
        .arg(arg!(-c --context [CONTEXT]).required(false).require_equals(false).help("Kubernetes Context"))
        .arg(arg!(-n --namespace [NAMESPACE]).required(false).require_equals(false).help("Kubernetes Namespace"))
        .arg(arg!([FORWARD]).num_args(1..).required(true).help("[[LOCAL_ADDRESS:]LOCAL_PORT:]service:port"))
}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("service is referencing `{0:#?}` in pod - but this does not exist on the pod")]
    CouldNotFindPort(IntOrString),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _matches = cli().get_matches();


    return Ok(());
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().skip(1).collect();
    info!("args, {:#?}", args);

    let client = Client::try_default().await?;
    let service_api: Api<Service> = Api::default_namespaced(client.clone());

    let mut handles = Vec::<JoinHandle<anyhow::Result<()>>>::with_capacity(args.len());

    let mut i = 0;
    for arg in args {
        let local_address = [127, 0, 0, 1];
        let mut local_port = Option::<u16>::None;
        let service_name;
        let service_port_arg;

        let bits: Vec<&str> = arg.split(':').collect();
        if bits.len() == 4 {
            // todo parse local address
            local_port = bits[1].parse::<u16>()?.into();
            service_name = bits[2];
            service_port_arg = bits[3];
        } else if bits.len() == 3 {
            local_port = bits[0].parse::<u16>()?.into();
            service_name = bits[1];
            service_port_arg = bits[2];
        } else if bits.len() == 2 {
            service_name = bits[0];
            service_port_arg = bits[1];
        } else {
            panic!("{} is incorrectly formatted", arg);
        }

        let service = service_api.get(service_name).await?;
        let service_spec = service.spec.unwrap();
        let selector = service_spec.selector.unwrap();

        let mut pod_port: Option<IntOrString> = None;
        if let Ok(pod_int) = service_port_arg.parse::<i32>() {
            pod_port = IntOrString::Int(pod_int).into();
        }

        if pod_port == None {
            pod_port = service_spec
                .ports
                .map(|pl| {
                    pl.into_iter()
                        .find(|p| p.name == Some(service_port_arg.to_string()))
                })
                .flatten()
                .map(|p| p.target_port.unwrap_or(IntOrString::Int(p.port)))
        }

        if pod_port == None {
            panic!(
                "could not find port {} on service {}",
                service_port_arg, service_name
            )
        }

        if local_port == None {
            if let IntOrString::Int(p) = pod_port.as_ref().unwrap() {
                local_port = (*p).try_into().ok()
            }
        }

        if local_port == None {
            panic!(
                "local port not provided, or service port is not a number for {}",
                arg
            )
        }

        let ppstr = pod_port.as_ref().map(|p| match p {
            IntOrString::Int(i) => i.to_string(),
            IntOrString::String(s) => s.to_string(),
        });

        let addr = SocketAddr::from((local_address, local_port.unwrap()));
        info!(local_addr = %addr, pod_port = ppstr, "forwarding traffic to the pod");
        info!(
            "try opening http://{0} in a browser, or `curl http://{0}`",
            addr
        );

        handles.insert(
            i,
            tokio::spawn(listen(
                client.clone(),
                selector_into_list_params(&selector),
                addr,
                pod_port.unwrap().clone(),
            )),
        );
        i = i + 1;
    }
    info!("use Ctrl-C to stop the server and delete the pod");

    futures::future::join_all(handles).await;

    Ok(())
}

async fn listen(
    client: Client,
    selector: ListParams,
    addr: SocketAddr,
    pod_port: IntOrString,
) -> anyhow::Result<()> {
    TcpListenerStream::new(TcpListener::bind(addr).await.unwrap())
        .take_until(tokio::signal::ctrl_c())
        .try_for_each(|client_conn| async {
            if let Ok(peer_addr) = client_conn.peer_addr() {
                info!(%peer_addr, "new connection");
            }

            let pod_api: Api<Pod> = Api::default_namespaced(client.clone());

            let matching_pods = pod_api.list(&selector).await.unwrap();
            let pod = matching_pods
                .items
                .into_iter()
                .filter(|p| {
                    p.status.as_ref().map_or(false, |s| {
                        s.conditions.as_ref().map_or(false, |cs| {
                            cs.into_iter()
                                .any(|c| c.type_ == "Ready" && c.status == "True")
                        })
                    })
                })
                .next()
                .unwrap();

            let port_int: Option<u16> = match pod_port.clone() {
                IntOrString::Int(i) => u16::try_from(i).ok(),
                IntOrString::String(n) => pod
                    .spec
                    .map(|pd| {
                        pd.containers
                            .into_iter()
                            .map(|c| c.ports)
                            .flatten()
                            .flat_map(|p| p)
                            .find(|p| p.name == Some(n.clone()))
                    })
                    .flatten()
                    .map(|p| u16::try_from(p.container_port).ok())
                    .flatten(),
            };

            if let Some(port) = port_int {
                tokio::spawn(async move {
                    if let Err(e) = forward_connection(
                        &pod_api,
                        pod.metadata.name.unwrap().as_str(),
                        port,
                        client_conn,
                    )
                    .await
                    {
                        error!(
                            error = e.as_ref() as &dyn std::error::Error,
                            "failed to forward connection"
                        );
                    }
                });
            } else {
                let e: &dyn std::error::Error = &MyError::CouldNotFindPort(pod_port.clone());
                error!(error = e, "failed to forward connection");
            }

            // keep the server running
            Ok(())
        })
        .await?;
    info!("done");
    Ok(())
}

fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> ListParams {
    let labels = selectors
        .iter()
        .fold(String::new(), |mut res, (key, value)| {
            if res.len() > 0 {
                res.push(',');
            }
            res.push_str(key);
            res.push('=');
            res.push_str(value);
            res
        });

    ListParams::default().labels(&labels)
}

async fn forward_connection(
    pods: &Api<Pod>,
    pod_name: &str,
    port: u16,
    mut client_conn: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let mut forwarder = pods.portforward(pod_name, &[port]).await?;
    let mut upstream_conn = forwarder
        .take_stream(port)
        .context("port not found in forwarder")?;
    tokio::io::copy_bidirectional(&mut client_conn, &mut upstream_conn).await?;
    drop(upstream_conn);
    forwarder.join().await?;
    info!("connection closed");
    Ok(())
}
