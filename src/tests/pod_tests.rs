//! Port selection and pod selection logic tests

#[cfg(test)]
mod tests {
    use crate::errors::MyError;
    use crate::pod::find_pod_port;
    use k8s_openapi::{
        api::core::v1::{Container, ContainerPort, Pod, PodCondition, PodSpec, PodStatus},
        apimachinery::pkg::{apis::meta::v1::ObjectMeta, util::intstr::IntOrString},
    };

    fn create_mock_pod(name: &str, ready: bool, ports: Option<Vec<ContainerPort>>) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some("test-namespace".to_string()),
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
                    last_transition_time: None,
                    message: None,
                    reason: None,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn create_standard_ports() -> Vec<ContainerPort> {
        vec![
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
                name: Some("metrics".to_string()),
                container_port: 9000,
                protocol: Some("TCP".to_string()),
                ..Default::default()
            },
        ]
    }

    mod port_resolution_tests {
        use super::*;

        #[test]
        fn numeric_port_resolution_success() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::Int(8080);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn numeric_port_zero_allowed() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::Int(0);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 0);
        }

        #[test]
        fn numeric_port_max_value() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::Int(65535);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 65535);
        }

        #[test]
        fn numeric_port_out_of_range() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::Int(65536);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn negative_port_fails() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::Int(-100);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn named_port_http_success() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn named_port_grpc_success() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::String("grpc".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 9090);
        }

        #[test]
        fn named_port_metrics_success() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::String("metrics".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 9000);
        }

        #[test]
        fn named_port_not_found() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::String("nonexistent".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn named_port_case_sensitive() {
            let pod = create_mock_pod("test-pod", true, Some(create_standard_ports()));
            let port_spec = IntOrString::String("HTTP".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn named_port_with_no_ports_defined() {
            let pod = create_mock_pod("test-pod", true, None);
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn named_port_with_empty_ports() {
            let pod = create_mock_pod("test-pod", true, Some(vec![]));
            let port_spec = IntOrString::String("http".to_string());

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }

        #[test]
        fn numeric_port_with_no_ports_defined() {
            let pod = create_mock_pod("test-pod", true, None);
            let port_spec = IntOrString::Int(8080);

            let result = find_pod_port(&port_spec, &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);
        }
    }

    mod pod_readiness_tests {
        use super::*;

        #[test]
        fn pod_ready_condition_true() {
            let pod = create_mock_pod("ready-pod", true, Some(create_standard_ports()));

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
        fn pod_ready_condition_false() {
            let pod = create_mock_pod("not-ready-pod", false, Some(create_standard_ports()));

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
            let mut pod = create_mock_pod("no-status-pod", true, Some(create_standard_ports()));
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

        #[test]
        fn pod_without_conditions() {
            let mut pod = create_mock_pod("no-conditions-pod", true, Some(create_standard_ports()));
            if let Some(ref mut status) = pod.status {
                status.conditions = None;
            }

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
        fn pod_with_empty_conditions() {
            let mut pod =
                create_mock_pod("empty-conditions-pod", true, Some(create_standard_ports()));
            if let Some(ref mut status) = pod.status {
                status.conditions = Some(vec![]);
            }

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
        fn pod_with_multiple_conditions() {
            let mut pod =
                create_mock_pod("multi-conditions-pod", false, Some(create_standard_ports()));
            if let Some(ref mut status) = pod.status {
                status.conditions = Some(vec![
                    PodCondition {
                        type_: "Initialized".to_string(),
                        status: "True".to_string(),
                        ..Default::default()
                    },
                    PodCondition {
                        type_: "Ready".to_string(),
                        status: "True".to_string(),
                        ..Default::default()
                    },
                    PodCondition {
                        type_: "ContainersReady".to_string(),
                        status: "True".to_string(),
                        ..Default::default()
                    },
                ]);
            }

            let is_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map_or(false, |cs| {
                    cs.iter().any(|c| c.type_ == "Ready" && c.status == "True")
                });

            assert!(is_ready);
        }
    }

    mod port_edge_cases {
        use super::*;

        #[test]
        fn port_with_unnamed_container_port() {
            let ports = vec![
                ContainerPort {
                    name: None, // Unnamed port
                    container_port: 8080,
                    protocol: Some("TCP".to_string()),
                    ..Default::default()
                },
                ContainerPort {
                    name: Some("named".to_string()),
                    container_port: 9090,
                    protocol: Some("TCP".to_string()),
                    ..Default::default()
                },
            ];

            let pod = create_mock_pod("test-pod", true, Some(ports));

            // Numeric port should work regardless of name
            let result = find_pod_port(&IntOrString::Int(8080), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);

            // Named port should only find the named one
            let result = find_pod_port(&IntOrString::String("named".to_string()), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 9090);
        }

        #[test]
        fn multiple_containers_same_port_name() {
            let mut pod = create_mock_pod("multi-container-pod", true, None);
            pod.spec = Some(PodSpec {
                containers: vec![
                    Container {
                        name: "container-1".to_string(),
                        ports: Some(vec![ContainerPort {
                            name: Some("http".to_string()),
                            container_port: 8080,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        }]),
                        ..Default::default()
                    },
                    Container {
                        name: "container-2".to_string(),
                        ports: Some(vec![ContainerPort {
                            name: Some("http".to_string()),
                            container_port: 9080,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        }]),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            });

            // Should find the first matching port
            let result = find_pod_port(&IntOrString::String("http".to_string()), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);
        }

        #[test]
        fn port_with_different_protocols() {
            let ports = vec![
                ContainerPort {
                    name: Some("tcp-port".to_string()),
                    container_port: 8080,
                    protocol: Some("TCP".to_string()),
                    ..Default::default()
                },
                ContainerPort {
                    name: Some("udp-port".to_string()),
                    container_port: 8081,
                    protocol: Some("UDP".to_string()),
                    ..Default::default()
                },
            ];

            let pod = create_mock_pod("protocol-test-pod", true, Some(ports));

            let result = find_pod_port(&IntOrString::String("tcp-port".to_string()), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);

            let result = find_pod_port(&IntOrString::String("udp-port".to_string()), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8081);
        }

        #[test]
        fn pod_spec_missing() {
            let mut pod = create_mock_pod("no-spec-pod", true, Some(create_standard_ports()));
            pod.spec = None;

            // Numeric port should still work
            let result = find_pod_port(&IntOrString::Int(8080), &pod);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 8080);

            // Named port should fail
            let result = find_pod_port(&IntOrString::String("http".to_string()), &pod);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), MyError::CouldNotFindPort(_)));
        }
    }

    mod service_port_mapping_tests {
        use super::*;

        #[test]
        fn standard_web_ports() {
            let pod = create_mock_pod(
                "web-pod",
                true,
                Some(vec![
                    ContainerPort {
                        name: Some("http".to_string()),
                        container_port: 80,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                    ContainerPort {
                        name: Some("https".to_string()),
                        container_port: 443,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                ]),
            );

            assert_eq!(
                find_pod_port(&IntOrString::String("http".to_string()), &pod).unwrap(),
                80
            );
            assert_eq!(
                find_pod_port(&IntOrString::String("https".to_string()), &pod).unwrap(),
                443
            );
        }

        #[test]
        fn database_ports() {
            let pod = create_mock_pod(
                "db-pod",
                true,
                Some(vec![
                    ContainerPort {
                        name: Some("mysql".to_string()),
                        container_port: 3306,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                    ContainerPort {
                        name: Some("postgres".to_string()),
                        container_port: 5432,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                    ContainerPort {
                        name: Some("redis".to_string()),
                        container_port: 6379,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                ]),
            );

            assert_eq!(
                find_pod_port(&IntOrString::String("mysql".to_string()), &pod).unwrap(),
                3306
            );
            assert_eq!(
                find_pod_port(&IntOrString::String("postgres".to_string()), &pod).unwrap(),
                5432
            );
            assert_eq!(
                find_pod_port(&IntOrString::String("redis".to_string()), &pod).unwrap(),
                6379
            );
        }

        #[test]
        fn kubernetes_standard_ports() {
            let pod = create_mock_pod(
                "k8s-pod",
                true,
                Some(vec![
                    ContainerPort {
                        name: Some("health".to_string()),
                        container_port: 8090,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                    ContainerPort {
                        name: Some("metrics".to_string()),
                        container_port: 9090,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                    ContainerPort {
                        name: Some("admin".to_string()),
                        container_port: 8080,
                        protocol: Some("TCP".to_string()),
                        ..Default::default()
                    },
                ]),
            );

            assert_eq!(
                find_pod_port(&IntOrString::String("health".to_string()), &pod).unwrap(),
                8090
            );
            assert_eq!(
                find_pod_port(&IntOrString::String("metrics".to_string()), &pod).unwrap(),
                9090
            );
            assert_eq!(
                find_pod_port(&IntOrString::String("admin".to_string()), &pod).unwrap(),
                8080
            );
        }
    }
}
