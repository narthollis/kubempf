use anyhow::Context;
// Example to listen on port 8080 locally, forwarding to port 80 in the example pod.
// Similar to `kubectl port-forward pod/example 8080:80`.
use futures::{StreamExt, TryStreamExt};
use std::{collections::BTreeMap, net::SocketAddr};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tokio_stream::wrappers::TcpListenerStream;
use tracing::*;

use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use kube::{
    api::{Api, ListParams},
    runtime::wait::{await_condition, conditions::is_pod_running},
    Client, ResourceExt,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let client = Client::try_default().await?;

    let pods: Api<Pod> = Api::default_namespaced(client.clone());
    let services: Api<Service> = Api::default_namespaced(client.clone());

    let service = services.get("example").await?;
    let selector = service.spec.and_then(|s| s.selector).unwrap();

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let pod_port = 80;
    info!(local_addr = %addr, pod_port, "forwarding traffic to the pod");
    info!(
        "try opening http://{0} in a browser, or `curl http://{0}`",
        addr
    );
    info!("use Ctrl-C to stop the server and delete the pod");

    tokio::spawn(listen(client, &selector, addr));


    Ok(())
}

async fn listen(client: Client, selector: &BTreeMap<String, String>, addr: SocketAddr) -> anyhow::Result<()> {
    TcpListenerStream::new(TcpListener::bind(addr).await.unwrap())
        .take_until(tokio::signal::ctrl_c())
        .try_for_each(|client_conn| async {
            if let Ok(peer_addr) = client_conn.peer_addr() {
                info!(%peer_addr, "new connection");
            }

            let pApi: Api<Pod> = Api::default_namespaced(client.clone());

            let lp = selector_into_list_params(&selector);
            let podList = pApi.list(&lp).await?;

            podList.iter()
                .filter(|p| p.status.and_then(|s| s.conditions).and_then(|cc| cc.iter().any(|c| c.status == "Ready" && c)))

            tokio::spawn(async move {
                if let Err(e) = forward_connection(&pApi, "example", 80, client_conn).await {
                    error!(
                        error = e.as_ref() as &dyn std::error::Error,
                        "failed to forward connection"
                    );
                }
            });
            // keep the server running
            Ok(())
        }).await?;
    Ok(())
}

fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> ListParams {
    let labels = selectors
        .iter()
        .fold(String::new(), |mut res, (key, value)| {
            if (res.len() > 0) {
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
