use crate::{cancelable_stream::CancelableReadWrite, cli::ControlArgs};
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

async fn find_pod(
    api: &Api<Pod>,
    selector: &ListParams,
    ignore_readiness: bool,
    randomise: bool,
) -> anyhow::Result<Pod> {
    let items = api.list(selector).await?.items;
    let length = items.len();

    let mut valid = items.into_iter().filter(|p| {
        ignore_readiness
            || p.status.as_ref().map_or(false, |s| {
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

pub fn find_pod_port(pod_port: &IntOrString, pod: &Pod) -> Result<u16, MyError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, PodCondition, PodSpec, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn create_test_pod(name: &str, ready: bool) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "test-container".to_string(),
                    ports: Some(vec![
                        ContainerPort {
                            name: Some("http".to_string()),
                            container_port: 8080,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        },
                        ContainerPort {
                            name: Some("grpc".to_string()),
                            container_port: 9090,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        },
                        ContainerPort {
                            name: None,
                            container_port: 3000,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                conditions: Some(vec![PodCondition {
                    type_: "Ready".to_string(),
                    status: if ready { "True" } else { "False" }.to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn create_pod_without_ports(name: &str, ready: bool) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "test-container".to_string(),
                    ports: None,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                conditions: Some(vec![PodCondition {
                    type_: "Ready".to_string(),
                    status: if ready { "True" } else { "False" }.to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn create_pod_with_empty_ports(name: &str, ready: bool) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "test-container".to_string(),
                    ports: Some(vec![]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                conditions: Some(vec![PodCondition {
                    type_: "Ready".to_string(),
                    status: if ready { "True" } else { "False" }.to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    mod find_pod_port_tests {
        use super::*;

        #[test]
        fn numeric_port_success() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::Int(8080);

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn numeric_port_out_of_range_fails() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::Int(65536);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
        }

        #[test]
        fn negative_port_fails() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::Int(-1);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
        }

        #[test]
        fn named_port_http_success() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn named_port_grpc_success() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::String("grpc".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 9090);
        }

        #[test]
        fn named_port_not_found_fails() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::String("nonexistent".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn pod_without_ports_named_lookup_fails() {
            let pod = create_pod_without_ports("test-pod", true);
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
        }

        #[test]
        fn pod_with_empty_ports_named_lookup_fails() {
            let pod = create_pod_with_empty_ports("test-pod", true);
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
        }

        #[test]
        fn pod_without_ports_numeric_succeeds() {
            let pod = create_pod_without_ports("test-pod", true);
            let port_spec = IntOrString::Int(8080);

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn pod_with_empty_ports_numeric_succeeds() {
            let pod = create_pod_with_empty_ports("test-pod", true);
            let port_spec = IntOrString::Int(8080);

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn case_sensitive_port_names() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::String("HTTP".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
        }

        #[test]
        fn port_zero_succeeds() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::Int(0);

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 0);
        }

        #[test]
        fn max_valid_port_succeeds() {
            let pod = create_test_pod("test-pod", true);
            let port_spec = IntOrString::Int(65535);

            let result = find_pod_port(&port_spec, &pod);
            assert_eq!(result.unwrap(), 65535);
        }
    }

    mod find_pod_tests {
        use super::*;

        #[tokio::test]
        async fn find_pod_ready_first() {
            let pods = vec![
                create_test_pod("pod-1", true),
                create_test_pod("pod-2", false),
                create_test_pod("pod-3", true),
            ];

            let result = find_ready_pod(&pods, false, false);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().metadata.name, Some("pod-1".to_string()));
        }

        #[tokio::test]
        async fn find_pod_ignore_readiness() {
            let pods = vec![
                create_test_pod("pod-1", false),
                create_test_pod("pod-2", false),
                create_test_pod("pod-3", true),
            ];

            let result = find_ready_pod(&pods, true, false);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().metadata.name, Some("pod-1".to_string()));
        }

        #[tokio::test]
        async fn find_pod_no_ready_pods_fails() {
            let pods = vec![
                create_test_pod("pod-1", false),
                create_test_pod("pod-2", false),
            ];

            let result = find_ready_pod(&pods, false, false);
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                MyError::MatchingReadyPodNotFound()
            ));
        }

        #[tokio::test]
        async fn find_pod_empty_list_fails() {
            let pods = vec![];

            let result = find_ready_pod(&pods, false, false);
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn find_pod_random_selection() {
            let pods = vec![
                create_test_pod("pod-1", true),
                create_test_pod("pod-2", true),
                create_test_pod("pod-3", true),
            ];

            let mut selected_pods = std::collections::HashSet::new();

            for _ in 0..50 {
                let result = find_ready_pod(&pods, false, true);
                assert!(result.is_ok());
                selected_pods.insert(result.unwrap().metadata.name.unwrap());
            }

            assert!(
                selected_pods.len() > 1,
                "Random selection should pick different pods"
            );
        }

        fn find_ready_pod(
            pods: &[Pod],
            ignore_readiness: bool,
            randomise: bool,
        ) -> Result<Pod, MyError> {
            let length = pods.len();
            if length == 0 {
                return Err(MyError::MatchingReadyPodNotFound());
            }

            let mut valid = pods.iter().filter(|p| {
                ignore_readiness
                    || p.status.as_ref().map_or(false, |s| {
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
                .cloned()
                .ok_or(MyError::MatchingReadyPodNotFound())
        }
    }

    mod pod_condition_tests {
        use super::*;

        #[test]
        fn pod_with_ready_true_condition() {
            let pod = create_test_pod("test-pod", true);

            let is_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                });

            assert!(is_ready);
        }

        #[test]
        fn pod_with_ready_false_condition() {
            let pod = create_test_pod("test-pod", false);

            let is_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                });

            assert!(!is_ready);
        }

        #[test]
        fn pod_without_conditions() {
            let mut pod = create_test_pod("test-pod", true);
            pod.status = Some(PodStatus {
                conditions: None,
                ..Default::default()
            });

            let is_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                });

            assert!(!is_ready);
        }

        #[test]
        fn pod_without_status() {
            let mut pod = create_test_pod("test-pod", true);
            pod.status = None;

            let is_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                });

            assert!(!is_ready);
        }
    }
}
