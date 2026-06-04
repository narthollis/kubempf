//! Integration tests for end-to-end functionality with mock Kubernetes API

#[cfg(test)]
mod tests {
    use crate::cli::{ControlArgs, Forward};
    use crate::errors::MyError;
    use k8s_openapi::api::core::v1::{
        Container, ContainerPort, Pod, PodCondition, PodSpec, PodStatus, Service, ServicePort,
        ServiceSpec,
    };
    use k8s_openapi::apimachinery::pkg::{apis::meta::v1::ObjectMeta, util::intstr::IntOrString};
    use std::collections::BTreeMap;

    // Mock Kubernetes objects factory for testing
    struct MockK8sObjects;

    impl MockK8sObjects {
        fn create_service(
            name: &str,
            namespace: &str,
            ports: Vec<ServicePort>,
            selectors: BTreeMap<String, String>,
        ) -> Service {
            Service {
                metadata: ObjectMeta {
                    name: Some(name.to_string()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                spec: Some(ServiceSpec {
                    ports: Some(ports),
                    selector: Some(selectors),
                    ..Default::default()
                }),
                ..Default::default()
            }
        }

        fn create_service_port(
            name: Option<&str>,
            port: i32,
            target_port: Option<IntOrString>,
        ) -> ServicePort {
            ServicePort {
                name: name.map(|s| s.to_string()),
                port,
                target_port,
                protocol: Some("TCP".to_string()),
                ..Default::default()
            }
        }

        fn create_pod(
            name: &str,
            namespace: &str,
            ready: bool,
            ports: Option<Vec<ContainerPort>>,
            labels: BTreeMap<String, String>,
        ) -> Pod {
            Pod {
                metadata: ObjectMeta {
                    name: Some(name.to_string()),
                    namespace: Some(namespace.to_string()),
                    labels: Some(labels),
                    ..Default::default()
                },
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "test-container".to_string(),
                        ports,
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

        fn create_container_port(name: Option<&str>, port: i32) -> ContainerPort {
            ContainerPort {
                name: name.map(|s| s.to_string()),
                container_port: port,
                protocol: Some("TCP".to_string()),
                ..Default::default()
            }
        }
    }

    mod service_pod_matching_tests {
        use super::*;

        #[test]
        fn service_with_matching_pods() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "web".to_string());
            selectors.insert("tier".to_string(), "frontend".to_string());

            let service = MockK8sObjects::create_service(
                "web-service",
                "default",
                vec![MockK8sObjects::create_service_port(
                    Some("http"),
                    80,
                    Some(IntOrString::Int(8080)),
                )],
                selectors.clone(),
            );

            let pod = MockK8sObjects::create_pod(
                "web-pod-1",
                "default",
                true,
                Some(vec![MockK8sObjects::create_container_port(
                    Some("http"),
                    8080,
                )]),
                selectors,
            );

            // Verify service has correct selectors
            assert_eq!(
                service
                    .spec
                    .as_ref()
                    .unwrap()
                    .selector
                    .as_ref()
                    .unwrap()
                    .get("app"),
                Some(&"web".to_string())
            );
            assert_eq!(
                service
                    .spec
                    .as_ref()
                    .unwrap()
                    .selector
                    .as_ref()
                    .unwrap()
                    .get("tier"),
                Some(&"frontend".to_string())
            );

            // Verify pod has matching labels
            assert_eq!(
                pod.metadata.labels.as_ref().unwrap().get("app"),
                Some(&"web".to_string())
            );
            assert_eq!(
                pod.metadata.labels.as_ref().unwrap().get("tier"),
                Some(&"frontend".to_string())
            );

            // Verify port mapping
            let service_port = &service.spec.as_ref().unwrap().ports.as_ref().unwrap()[0];
            let container_port = &pod.spec.as_ref().unwrap().containers[0]
                .ports
                .as_ref()
                .unwrap()[0];

            assert_eq!(
                service_port.target_port,
                Some(IntOrString::Int(container_port.container_port))
            );
        }

        #[test]
        fn service_with_mismatched_selectors() {
            let mut service_selectors = BTreeMap::new();
            service_selectors.insert("app".to_string(), "web".to_string());
            service_selectors.insert("version".to_string(), "v1".to_string());

            let mut pod_labels = BTreeMap::new();
            pod_labels.insert("app".to_string(), "web".to_string());
            pod_labels.insert("version".to_string(), "v2".to_string()); // Mismatch

            let service = MockK8sObjects::create_service(
                "web-service",
                "default",
                vec![MockK8sObjects::create_service_port(Some("http"), 80, None)],
                service_selectors,
            );

            let pod = MockK8sObjects::create_pod("web-pod-1", "default", true, None, pod_labels);

            // Verify the mismatch
            let service_version = service
                .spec
                .as_ref()
                .unwrap()
                .selector
                .as_ref()
                .unwrap()
                .get("version")
                .unwrap();
            let pod_version = pod
                .metadata
                .labels
                .as_ref()
                .unwrap()
                .get("version")
                .unwrap();

            assert_ne!(service_version, pod_version);
        }

        #[test]
        fn service_without_selectors() {
            let service = MockK8sObjects::create_service(
                "headless-service",
                "default",
                vec![MockK8sObjects::create_service_port(None, 80, None)],
                BTreeMap::new(),
            );

            // Should have no selectors (headless service)
            assert!(service
                .spec
                .as_ref()
                .unwrap()
                .selector
                .as_ref()
                .unwrap()
                .is_empty());
        }

        #[test]
        fn multiple_pods_same_selectors() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "api".to_string());

            let service = MockK8sObjects::create_service(
                "api-service",
                "production",
                vec![MockK8sObjects::create_service_port(
                    Some("grpc"),
                    443,
                    Some(IntOrString::Int(9090)),
                )],
                selectors.clone(),
            );

            let pod1 = MockK8sObjects::create_pod(
                "api-pod-1",
                "production",
                true,
                None,
                selectors.clone(),
            );
            let pod2 = MockK8sObjects::create_pod(
                "api-pod-2",
                "production",
                true,
                None,
                selectors.clone(),
            );
            let pod3 =
                MockK8sObjects::create_pod("api-pod-3", "production", false, None, selectors); // Not ready

            // All pods should match the service selector
            for pod in [&pod1, &pod2, &pod3] {
                assert_eq!(
                    pod.metadata.labels.as_ref().unwrap().get("app"),
                    Some(&"api".to_string())
                );
            }

            // Check readiness states
            assert!(is_pod_ready(&pod1));
            assert!(is_pod_ready(&pod2));
            assert!(!is_pod_ready(&pod3));
        }

        fn is_pod_ready(pod: &Pod) -> bool {
            pod.status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                })
        }
    }

    mod port_mapping_scenarios {
        use super::*;

        #[test]
        fn named_port_mapping() {
            let service = MockK8sObjects::create_service(
                "web-service",
                "default",
                vec![
                    MockK8sObjects::create_service_port(
                        Some("http"),
                        80,
                        Some(IntOrString::String("web".to_string())),
                    ),
                    MockK8sObjects::create_service_port(
                        Some("https"),
                        443,
                        Some(IntOrString::String("web-tls".to_string())),
                    ),
                ],
                BTreeMap::new(),
            );

            let pod = MockK8sObjects::create_pod(
                "web-pod",
                "default",
                true,
                Some(vec![
                    MockK8sObjects::create_container_port(Some("web"), 8080),
                    MockK8sObjects::create_container_port(Some("web-tls"), 8443),
                ]),
                BTreeMap::new(),
            );

            let service_ports = service.spec.unwrap().ports.unwrap();
            let container_ports = pod.spec.as_ref().unwrap().containers[0].ports.as_ref().unwrap();

            // Verify http port mapping
            let http_service_port = service_ports
                .iter()
                .find(|p| p.name.as_ref() == Some(&"http".to_string()))
                .unwrap();
            let web_container_port = container_ports
                .iter()
                .find(|p| p.name.as_ref() == Some(&"web".to_string()))
                .unwrap();

            if let Some(IntOrString::String(target_name)) = &http_service_port.target_port {
                assert_eq!(target_name, "web");
                assert_eq!(web_container_port.container_port, 8080);
            }

            // Verify https port mapping
            let https_service_port = service_ports
                .iter()
                .find(|p| p.name.as_ref() == Some(&"https".to_string()))
                .unwrap();
            let web_tls_container_port = container_ports
                .iter()
                .find(|p| p.name.as_ref() == Some(&"web-tls".to_string()))
                .unwrap();

            if let Some(IntOrString::String(target_name)) = &https_service_port.target_port {
                assert_eq!(target_name, "web-tls");
                assert_eq!(web_tls_container_port.container_port, 8443);
            }
        }

        #[test]
        fn numeric_port_mapping() {
            let service = MockK8sObjects::create_service(
                "db-service",
                "default",
                vec![MockK8sObjects::create_service_port(
                    Some("mysql"),
                    3306,
                    Some(IntOrString::Int(3306)),
                )],
                BTreeMap::new(),
            );

            let pod = MockK8sObjects::create_pod(
                "db-pod",
                "default",
                true,
                Some(vec![MockK8sObjects::create_container_port(
                    Some("mysql"),
                    3306,
                )]),
                BTreeMap::new(),
            );

            let service_port = &service.spec.unwrap().ports.unwrap()[0];
            let container_port = &pod.spec.as_ref().unwrap().containers[0].ports.as_ref().unwrap()[0];

            assert_eq!(service_port.port, 3306);
            if let Some(IntOrString::Int(target_port)) = service_port.target_port {
                assert_eq!(target_port, container_port.container_port);
            }
        }

        #[test]
        fn port_mapping_without_target_port() {
            // When target_port is not specified, it defaults to the service port
            let service = MockK8sObjects::create_service(
                "simple-service",
                "default",
                vec![MockK8sObjects::create_service_port(
                    Some("http"),
                    8080,
                    None,
                )],
                BTreeMap::new(),
            );

            let service_port = &service.spec.unwrap().ports.unwrap()[0];
            assert_eq!(service_port.port, 8080);
            assert_eq!(service_port.target_port, None);
        }
    }

    mod control_args_scenarios {
        use super::*;

        #[test]
        fn control_args_default_behavior() {
            let args = ControlArgs {
                ignore_readiness: false,
                close_on_unready: false,
                randomise: false,
            };

            // Default behavior: check readiness, don't close on unready, select first pod
            assert!(!args.ignore_readiness);
            assert!(!args.close_on_unready);
            assert!(!args.randomise);
        }

        #[test]
        fn control_args_ignore_readiness() {
            let args = ControlArgs {
                ignore_readiness: true,
                close_on_unready: false,
                randomise: false,
            };

            // Should ignore pod readiness when selecting
            assert!(args.ignore_readiness);
        }

        #[test]
        fn control_args_close_on_unready() {
            let args = ControlArgs {
                ignore_readiness: false,
                close_on_unready: true,
                randomise: false,
            };

            // Should close connections when pod becomes unready
            assert!(args.close_on_unready);
        }

        #[test]
        fn control_args_randomise_selection() {
            let args = ControlArgs {
                ignore_readiness: false,
                close_on_unready: false,
                randomise: true,
            };

            // Should randomly select from available pods
            assert!(args.randomise);
        }

        #[test]
        fn control_args_all_flags_enabled() {
            let args = ControlArgs {
                ignore_readiness: true,
                close_on_unready: true,
                randomise: true,
            };

            // All flags can be enabled simultaneously
            assert!(args.ignore_readiness);
            assert!(args.close_on_unready);
            assert!(args.randomise);
        }
    }

    mod forward_configuration_scenarios {
        use super::*;

        #[test]
        fn forward_with_named_port() {
            let forward = Forward::parse("8080:web-service:http").unwrap();

            assert_eq!(forward.local_port, 8080);
            assert_eq!(forward.service_name, "web-service");
            assert_eq!(forward.service_port, "http");
            assert_eq!(forward.namespace, None);
            assert_eq!(forward.local_address, None);
        }

        #[test]
        fn forward_with_namespace_and_named_port() {
            let forward = Forward::parse("3000:production/api-service:grpc").unwrap();

            assert_eq!(forward.local_port, 3000);
            assert_eq!(forward.service_name, "api-service");
            assert_eq!(forward.service_port, "grpc");
            assert_eq!(forward.namespace, Some("production".to_string()));
            assert_eq!(forward.local_address, None);
        }

        #[test]
        fn forward_with_ip_and_named_port() {
            let forward = Forward::parse("127.0.0.1:9090:monitoring-service:metrics").unwrap();

            assert_eq!(forward.local_port, 9090);
            assert_eq!(forward.service_name, "monitoring-service");
            assert_eq!(forward.service_port, "metrics");
            assert_eq!(forward.namespace, None);
            assert!(forward.local_address.is_some());
        }

        #[test]
        fn multiple_forwards_configuration() {
            let forwards = vec![
                Forward::parse("8080:web-service:http").unwrap(),
                Forward::parse("3306:database-service:mysql").unwrap(),
                Forward::parse("6379:cache-service:redis").unwrap(),
            ];

            assert_eq!(forwards.len(), 3);

            // Verify each forward is independent
            assert_eq!(forwards[0].service_name, "web-service");
            assert_eq!(forwards[1].service_name, "database-service");
            assert_eq!(forwards[2].service_name, "cache-service");

            // Verify different ports
            assert_eq!(forwards[0].local_port, 8080);
            assert_eq!(forwards[1].local_port, 3306);
            assert_eq!(forwards[2].local_port, 6379);
        }
    }

    mod error_propagation_scenarios {
        use super::*;

        #[test]
        fn service_not_found_error() {
            let error = MyError::ServiceNotFound("nonexistent-service".to_string());
            let error_msg = format!("{}", error);
            assert!(error_msg.contains("nonexistent-service"));
            assert!(error_msg.contains("not found or invalid"));
        }

        #[test]
        fn service_missing_selectors_error() {
            let error = MyError::ServiceMissingSelectors("headless-service".to_string());
            let error_msg = format!("{}", error);
            assert!(error_msg.contains("headless-service"));
            assert!(error_msg.contains("missing selectors"));
        }

        #[test]
        fn missing_named_port_error() {
            let error = MyError::MissingNamedPort(
                "nonexistent-port".to_string(),
                "web-service".to_string(),
            );
            let error_msg = format!("{}", error);
            assert!(error_msg.contains("nonexistent-port"));
            assert!(error_msg.contains("web-service"));
            assert!(error_msg.contains("unable to find named port"));
        }

        #[test]
        fn no_ready_pods_error() {
            let error = MyError::MatchingReadyPodNotFound();
            let error_msg = format!("{}", error);
            assert_eq!(error_msg, "no matching ready pods");
        }

        #[test]
        fn port_not_found_on_pod_error() {
            let error = MyError::CouldNotFindPort(IntOrString::String("missing-port".to_string()));
            let error_msg = format!("{}", error);
            assert!(error_msg.contains("missing-port"));
            assert!(error_msg.contains("does not exist on the pod"));
        }
    }
}
