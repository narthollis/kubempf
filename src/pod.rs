use crate::{
    cancelable_stream::CancelableReadWrite,
    cli::ControlArgs,
};
use anyhow::Context;
use futures::future::Either;
use futures::{stream::AbortHandle, TryStreamExt};
use k8s_openapi::{
    api::core::v1::{ContainerPort, Pod},
    apimachinery::pkg::util::intstr::IntOrString,
};
use kube::{
    api::ListParams,
    runtime::{watcher, watcher::Config, WatchStreamExt},
    Api,
};
use rand::Rng;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::pin;
use tracing::{error, info, info_span, Instrument};

use crate::errors::MyError;

pub async fn forward_connection(
    pod_api: &Api<Pod>,
    selector: &ListParams,
    pod_port: &IntOrString,
    client_conn: impl AsyncRead + AsyncWrite + Unpin,
    args: ControlArgs,
) -> anyhow::Result<()> {
    let pod = find_pod(pod_api, selector, args.ignore_readiness, args.randomise).await?;
    let port = find_pod_port(pod_port, &pod)?;

    let name_string = pod.metadata.name.unwrap(); // how on earth you would end up here without a pod name is beyond me
    let pod_name = name_string.as_str();

    async move {
        let result = match args.close_on_unready {
            true => _forward_connection_with_unready(pod_api, pod_name, port, client_conn).await,
            false => _forward_connection(pod_api, pod_name, port, client_conn).await,
        };

        if let Err(e) = result {
            error!(
                error = e.as_ref() as &dyn std::error::Error,
                "an error occurred while forwarding the connection"
            );
        }
    }
    .instrument(info_span!(
        "pod",
        pod_name = pod_name.to_string(),
        pod_port = port
    ))
    .await;

    Ok(())
}

async fn _forward_connection(
    pod_api: &Api<Pod>,
    pod_name: &str,
    port: u16,
    mut client: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    info!("forwarding started");

    let mut forwarder = pod_api.portforward(pod_name, &[port]).await?;
    let mut upstream = forwarder
        .take_stream(port)
        .context("port not found in forwarder")?;

    let (up, down) = tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;

    forwarder.join().await.context("forwarder join error")?;

    info!(
        up = format!("{0:#}", byte_unit::Byte::from_u64(up)),
        down = format!("{0:#}", byte_unit::Byte::from_u64(down)),
        "forwarding finished"
    );

    Ok(())
}

async fn _forward_connection_with_unready(
    pod_api: &Api<Pod>,
    pod_name: &str,
    port: u16,
    mut client: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    info!("forwarding started");

    let mut forwarder = pod_api.portforward(pod_name, &[port]).await?;
    let mut upstream = forwarder
        .take_stream(port)
        .context("port not found in forwarder")?;

    let (abort_handle, abort_registration) = AbortHandle::new_pair();

    let unready = wait_for_unready(pod_api.clone(), pod_name, abort_registration.handle());

    let mut cancelable_upstream = CancelableReadWrite::new(&mut upstream, &abort_registration);
    let mut cancelable_client = CancelableReadWrite::new(&mut client, &abort_registration);

    let copy = tokio::io::copy_bidirectional(&mut cancelable_client, &mut cancelable_upstream);

    pin!(unready);
    pin!(copy);

    let (up, down) = match futures::future::select(copy, unready).await {
        Either::Left((left, _)) => {
            abort_handle.abort();
            left.context("copy_bidirectional")?
        }
        Either::Right((right, left)) => {
            abort_handle.abort();

            right.context("wait_for_unready")?;

            info!("closing connection due to pod transitioning to unready");

            left.await?
        }
    };

    forwarder.join().await.context("forwarder join error")?;

    info!(
        up = format!("{0:#}", byte_unit::Byte::from_u64(up)),
        down = format!("{0:#}", byte_unit::Byte::from_u64(down)),
        "forwarding finished"
    );

    Ok(())
}


async fn find_pod(api: &Api<Pod>, selector: &ListParams, ignore_readiness: bool, randomise: bool) -> anyhow::Result<Pod> {
    let items = api.list(selector).await?.items;
    let length = items.len();

    let mut valid = items
        .into_iter()
        .filter(|p| {
            ignore_readiness ||
            p.status.as_ref().map_or(false, |s| {
                s.conditions.as_ref().map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                })
            })
        });

    let count = match randomise {
        true => rand::rng().random_range(0..length),
        false => 0,
    };

    valid
        .nth(count)
        .ok_or_else(|| MyError::MatchingReadyPodNotFound().into())
}

const EMPTY_CONTAINER_LIST: &Vec<ContainerPort> = &vec![];

fn find_pod_port(pod_port: &IntOrString, pod: &Pod) -> Result<u16, MyError> {
    match pod_port {
        IntOrString::Int(i) => match u16::try_from(*i) {
            Ok(t) => Ok(t),
            Err(_) => Err(MyError::CouldNotFindPort(pod_port.clone())),
        },
        IntOrString::String(n) => pod
            .spec
            .as_ref()
            .and_then(|s| {
                s.containers
                    .iter()
                    .flat_map(|c| c.ports.as_ref().unwrap_or(EMPTY_CONTAINER_LIST))
                    .find(|p| p.name.as_ref().is_some_and(|v| v == n))
            })
            .and_then(|p| u16::try_from(p.container_port).ok())
            .ok_or(MyError::CouldNotFindPort(pod_port.clone())),
    }
}

async fn wait_for_unready(
    api: Api<Pod>,
    name: &str,
    abort_handle: AbortHandle,
) -> anyhow::Result<()> {
    //let mut stream  = watch_object(api, name.as_str());
    let stream = watcher(
        api,
        Config::default().fields(format!("metadata.name={}", name).as_str()),
    )
    .applied_objects();

    pin!(stream);

    while let Some(pod) = stream.try_next().await? {
        if abort_handle.is_aborted() {
            break;
        }
        if let Some(status) = pod.status {
            let is_ready = status.conditions.as_ref().map_or(false, |cs| {
                cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
            });
            if !is_ready {
                break;
            }
        }
    }

    Ok(())
}
