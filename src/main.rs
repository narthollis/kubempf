mod cancelable_stream;
pub(crate) mod cli;
pub(crate) mod errors;
mod pod;

use crate::{
    cli::{parse_args, Forward},
    errors::MyError,
};
use cli::ControlArgs;
use futures::{future::join_all, StreamExt, TryStreamExt};
use k8s_openapi::{api::core::v1::{Pod, Service}, apimachinery::pkg::util::intstr::IntOrString};
use kube::{
    api::{Api, ListParams},
    Client, Config,
};
use std::{collections::BTreeMap, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_stream::{wrappers::TcpListenerStream, StreamMap};
use tracing::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args();

    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_level(false)
        .with_target(false);

    if args.compact {
        tracing_subscriber::fmt()
            .event_format(format.compact())
            .with_max_level(tracing::Level::INFO)
            .init();
    } else {
        tracing_subscriber::fmt()
            .event_format(format.pretty().with_source_location(false))
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    let kube_opts = kube::config::KubeConfigOptions {
        context: args.context,
        cluster: None,
        user: None,
    };
    let mut config = Config::from_kubeconfig(&kube_opts).await?;
    if let Some(ns) = args.namespace {
        config.default_namespace = ns;
    }

    let client = Client::try_from(config)?;

    let handles: anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> =
        join_all(
                args.forwards
                    .iter()
                    .map(|forward| create_forward(client.clone(), forward, args.control.clone()))
            )
            .await
            .into_iter()
            .collect();

    info!("Ctrl-C to stop the server");
    join_all(handles?).await;

    Ok(())
}

fn get_service_api(namespace: Option<&String>, client: Client) -> Api<Service> {
    match namespace {
        Some(ns) => Api::namespaced(client, ns.as_str()),
        None => Api::default_namespaced(client),
    }
}

fn get_pod_api(namespace: Option<&String>, client: Client) -> Api<Pod> {
    match namespace {
        Some(ns) => Api::namespaced(client, ns.as_str()),
        None => Api::default_namespaced(client)
    }
}

async fn create_forward(
    client: Client,
    forward: &Forward,
    args: ControlArgs,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let default_namespace = client.default_namespace().to_owned();

    let service_api = get_service_api(forward.namespace.as_ref(), client);

    let service = service_api.get(forward.service_name.as_str()).await?;
    let service_spec = service
        .spec
        .ok_or_else(|| MyError::ServiceNotFound(forward.service_name.to_string()))?;
    let selector = service_spec
        .selector
        .ok_or_else(|| MyError::ServiceMissingSelectors(forward.service_name.to_string()))?;

    let pod_port: IntOrString = match forward.service_port.parse::<i32>() {
        Ok(p) => Ok(IntOrString::Int(p)),
        Err(_) => service_spec
            .ports
            .and_then(|pl| {
                pl.into_iter()
                    .find(|p| p.name == Some(forward.service_port.to_string()))
            })
            .map(|p| p.target_port.unwrap_or(IntOrString::Int(p.port)))
            .ok_or_else(|| {
                MyError::MissingNamedPort(
                    forward.service_port.to_string(),
                    forward.service_name.to_string(),
                )
            }),
    }?;

    let _forward_span = info_span!(
        "forward",
        target = format!(
            "{namespace}/{service_name}:{service_port}",
            namespace = forward.namespace.as_ref().unwrap_or(&default_namespace),
            service_name = forward.service_name,
            service_port = forward.service_port
        )
    )
    .entered();

    let addr = forward.local_address.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let sock_addr = SocketAddr::from((addr, forward.local_port));
    
    let socket = TcpListener::bind(sock_addr).await?;
    info!(local_addr = addr.to_string(), "bound");

    let socket_2 = match forward.local_address {
        Some(_) => None,
        None => {        
            let addr = forward.local_address.unwrap_or(IpAddr::V6(Ipv6Addr::LOCALHOST));
            let sock_addr = SocketAddr::from((addr, forward.local_port));
            
            let socket = TcpListener::bind(sock_addr).await?;
            info!(local_addr = addr.to_string(), "bound");

            Some(socket)
        }
    };

    Ok(tokio::spawn(
        serve(
            socket,
            socket_2,
            get_pod_api(forward.namespace.as_ref(), service_api.into_client()),
            selector_into_list_params(&selector),
            pod_port,
            args,
        )
        .in_current_span(),
    ))
}

async fn serve(
    socket: TcpListener,
    socket_2: Option<TcpListener>,
    pod_api: Api<Pod>,
    selector: ListParams,
    pod_port: IntOrString,
    args: ControlArgs,
) -> anyhow::Result<()> {
    let mut map = StreamMap::new();
    map.insert(0, TcpListenerStream::new(socket));

    if let Some(s) = socket_2 {
        map.insert(1, TcpListenerStream::new(s));       
    }    

    map
        .take_until(tokio::signal::ctrl_c())
        .map(|(_, x)| x)
        .try_for_each(|client_conn| async {
            let _connection_span = info_span!(
                "connection",
                peer_addr = client_conn.peer_addr()?.to_string()
            )
            .entered();

            trace!("accepted new connection");

            let sel = selector.clone();
            let port = pod_port.clone();

            let api = pod_api.clone();
            let args = args.clone();

            tokio::spawn(
                async move {
                    if let Err(e) = pod::forward_connection(&api, &sel, &port, client_conn, args).await {
                        error!(
                            error = e.as_ref() as &dyn std::error::Error,
                            "failed to forward connection"
                        );
                    }
                }
                .in_current_span(),
            );

            Ok(())
        })
        .await?;
    trace!("closed");
    Ok(())
}

fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> ListParams {
    let labels = selectors
        .iter()
        .fold(String::new(), |mut res, (key, value)| {
            if !res.is_empty() {
                res.push(',');
            }
            res.push_str(key);
            res.push('=');
            res.push_str(value);
            res
        });

    ListParams::default().labels(&labels)
}
