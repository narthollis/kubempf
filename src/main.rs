use anyhow::Context;
use futures::{StreamExt, TryStreamExt};
use std::{collections::BTreeMap, net::SocketAddr};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_stream::wrappers::TcpListenerStream;
use tracing::*;

use clap::{arg, Command};
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::util::intstr::IntOrString};
use kube::{
    api::{Api, ListParams},
    Client, Config,
};

fn cli() -> Command {
    Command::new("kubempf")
        .about("Multi-service port proxying tool for Kubernetes")
        .arg_required_else_help(true)
        .allow_external_subcommands(false)
        .arg(
            arg!(-c - -context[CONTEXT])
                .required(false)
                .require_equals(false)
                .help("Kubernetes Context"),
        )
        .arg(
            arg!(-n - -namespace[NAMESPACE])
                .required(false)
                .require_equals(false)
                .help("Kubernetes Namespace"),
        )
        .arg(
            arg!([FORWARD])
                .id("forwards")
                .num_args(1..)
                .required(true)
                .help("[[LOCAL_ADDRESS:]LOCAL_PORT:]service:port"),
        )
}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("unable to parse argument {0}")]
    ArgumentParseError(String),
    #[error("unable to find named port {0} on service {1}")]
    MissingNamedPort(String, String),
    #[error("unable to infer local port from named service port {0} for service {1}")]
    UnableToInferLocalPort(String, String),
    #[error("unable to convert service port {0} to u16 for service {1}")]
    UnableToConvertServicePort(String, String),
    #[error("service {0} not found or invalid")]
    ServiceNotFound(String),
    #[error("service {0} not compatiable as it is is missing selectors")]
    ServiceMissingSelectors(String),
    #[error("service is referencing `{0:#?}` in pod - but this does not exist on the pod")]
    CouldNotFindPort(IntOrString),
}

#[derive(Debug)]
struct Forward {
    service_name: String,
    service_port: String,
    pod_list_params: ListParams,
    pod_port: IntOrString,
    local_address: SocketAddr,
}

impl Forward {
    pub async fn parse(api: &Api<Service>, arg: &str) -> anyhow::Result<Forward> {
        let local_address = [127, 0, 0, 1];
        let local_port_arg;
        let service_name;
        let service_port_arg;

        let bits: Vec<&str> = (*arg).split(':').collect();
        if bits.len() == 4 {
            // todo parse local address
            local_port_arg = bits[1].parse::<u16>()?.into();
            service_name = bits[2];
            service_port_arg = bits[3];
        } else if bits.len() == 3 {
            local_port_arg = bits[0].parse::<u16>()?.into();
            service_name = bits[1];
            service_port_arg = bits[2];
        } else if bits.len() == 2 {
            local_port_arg = Option::<u16>::None;
            service_name = bits[0];
            service_port_arg = bits[1];
        } else {
            return Err(MyError::ArgumentParseError(arg.to_string()).into());
        }

        let service = api.get(service_name).await?;
        let service_spec = service.spec.ok_or(MyError::ServiceNotFound(service_name.to_string()))?;
        let selector = service_spec.selector.ok_or(MyError::ServiceMissingSelectors(service_name.to_string()))?;

        let pod_port: IntOrString = match service_port_arg.parse::<i32>() {
            Ok(p) => Ok(IntOrString::Int(p)),
            Err(_) => service_spec
                .ports
                .and_then(|pl| pl.into_iter().find(|p| p.name == Some(service_port_arg.to_string())))
                .map(|p| p.target_port.unwrap_or(IntOrString::Int(p.port)))
                .ok_or(MyError::MissingNamedPort(service_port_arg.to_string(), service_name.to_string()))
        }?;

        let local_port = match local_port_arg {
            Some(p) => Ok(p),
            None => match pod_port {
                IntOrString::Int(p) => (p).try_into().map_err(|_| MyError::UnableToConvertServicePort(p.to_string(), service_name.to_string())),
                IntOrString::String(_) => Err(MyError::UnableToInferLocalPort(service_port_arg.to_string(), service_name.to_string()).into())
            }
        }?;

        Ok(Self {
            service_name: service_name.to_string(),
            service_port: service_port_arg.to_string(),
            pod_list_params: selector_into_list_params(&selector),
            pod_port: pod_port,
            local_address: SocketAddr::from((local_address, local_port)),
        })
    }
}

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

    let mut i = 0;
    for arg in forwards {
        let forward = Forward::parse(&service_api, arg).await?;

        info!("Forwarding {} to {}:{}", forward.local_address, forward.service_name, forward.service_port);

        handles.insert(
            i,
            tokio::spawn(listen(
                client.clone(),
                forward.pod_list_params,
                forward.local_address,
                forward.pod_port,
            )),
        );
        i = i + 1;
    }

    info!("Ctrl-C to stop the server");
    futures::future::join_all(handles).await;

    Ok(())
}

async fn listen(
    client: Client,
    selector: ListParams,
    addr: SocketAddr,
    pod_port: IntOrString,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    TcpListenerStream::new(listener)
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
