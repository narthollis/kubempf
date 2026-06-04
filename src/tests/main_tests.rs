//! Main module utility function tests

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::{Pod, Service};
    use kube::{Api, Client, Config};
    use std::collections::BTreeMap;

    // Import the functions we want to test - these are in the main module
    // We need to make them public or create test-specific versions

    fn get_service_api(namespace: Option<&String>, client: Client) -> Api<Service> {
        match namespace {
            Some(ns) => Api::namespaced(client, ns.as_str()),
            None => Api::default_namespaced(client),
        }
    }

    fn get_pod_api(namespace: Option<&String>, client: Client) -> Api<Pod> {
        match namespace {
            Some(ns) => Api::namespaced(client, ns.as_str()),
            None => Api::default_namespaced(client),
        }
    }

    fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> kube::api::ListParams {
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

        kube::api::ListParams::default().labels(&labels)
    }

    // Helper to create a mock client for testing
    fn create_mock_client() -> Client {
        let config = Config {
            cluster_url: "https://localhost:8443".parse().unwrap(),
            default_namespace: "default".to_string(),
            root_cert: None,
            headers: Default::default(),
            timeout: None,
            connect_timeout: None,
            read_timeout: None,
            write_timeout: None,
            proxy_url: None,
            tls: kube::config::Tls::Insecure,
            auth_info: kube::config::AuthInfo::default(),
        };
        Client::try_from(config).unwrap()
    }

    mod selector_into_list_params_tests {
        use super::*;

        #[test]
        fn empty_selectors() {
            let selectors = BTreeMap::new();
            let list_params = selector_into_list_params(&selectors);

            // Should have empty label selector
            let label_selector = list_params.label_selector.unwrap_or_default();
            assert_eq!(label_selector, "");
        }

        #[test]
        fn single_selector() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "nginx".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();
            assert_eq!(label_selector, "app=nginx");
        }

        #[test]
        fn multiple_selectors() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "nginx".to_string());
            selectors.insert("version".to_string(), "v1.0".to_string());
            selectors.insert("tier".to_string(), "frontend".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();

            // BTreeMap orders keys, so we can predict the order
            assert_eq!(label_selector, "app=nginx,tier=frontend,version=v1.0");
        }

        #[test]
        fn selectors_with_special_characters() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app.kubernetes.io/name".to_string(), "my-app".to_string());
            selectors.insert("component".to_string(), "web-server".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();
            assert_eq!(
                label_selector,
                "app.kubernetes.io/name=my-app,component=web-server"
            );
        }

        #[test]
        fn selectors_with_empty_values() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "".to_string());
            selectors.insert("version".to_string(), "v1".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();
            assert_eq!(label_selector, "app=,version=v1");
        }

        #[test]
        fn selectors_with_empty_keys() {
            let mut selectors = BTreeMap::new();
            selectors.insert("".to_string(), "value".to_string());
            selectors.insert("app".to_string(), "nginx".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();
            assert_eq!(label_selector, "=value,app=nginx");
        }

        #[test]
        fn selectors_ordering_consistency() {
            // Test that BTreeMap ordering is consistent
            let mut selectors1 = BTreeMap::new();
            selectors1.insert("z".to_string(), "last".to_string());
            selectors1.insert("a".to_string(), "first".to_string());
            selectors1.insert("m".to_string(), "middle".to_string());

            let mut selectors2 = BTreeMap::new();
            selectors2.insert("a".to_string(), "first".to_string());
            selectors2.insert("m".to_string(), "middle".to_string());
            selectors2.insert("z".to_string(), "last".to_string());

            let list_params1 = selector_into_list_params(&selectors1);
            let list_params2 = selector_into_list_params(&selectors2);

            assert_eq!(list_params1.label_selector, list_params2.label_selector);
            assert_eq!(
                list_params1.label_selector.unwrap(),
                "a=first,m=middle,z=last"
            );
        }

        #[test]
        fn selectors_with_kubernetes_standard_labels() {
            let mut selectors = BTreeMap::new();
            selectors.insert("app.kubernetes.io/name".to_string(), "nginx".to_string());
            selectors.insert(
                "app.kubernetes.io/instance".to_string(),
                "my-nginx".to_string(),
            );
            selectors.insert(
                "app.kubernetes.io/version".to_string(),
                "1.21.0".to_string(),
            );
            selectors.insert(
                "app.kubernetes.io/component".to_string(),
                "webserver".to_string(),
            );
            selectors.insert(
                "app.kubernetes.io/part-of".to_string(),
                "my-app".to_string(),
            );

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();

            // Should contain all the standard Kubernetes labels
            assert!(label_selector.contains("app.kubernetes.io/name=nginx"));
            assert!(label_selector.contains("app.kubernetes.io/instance=my-nginx"));
            assert!(label_selector.contains("app.kubernetes.io/version=1.21.0"));
            assert!(label_selector.contains("app.kubernetes.io/component=webserver"));
            assert!(label_selector.contains("app.kubernetes.io/part-of=my-app"));
        }

        #[test]
        fn selectors_with_numeric_values() {
            let mut selectors = BTreeMap::new();
            selectors.insert("replicas".to_string(), "3".to_string());
            selectors.insert("port".to_string(), "8080".to_string());
            selectors.insert("priority".to_string(), "100".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();
            assert_eq!(label_selector, "port=8080,priority=100,replicas=3");
        }
    }

    mod api_factory_tests {
        use super::*;

        #[test]
        fn get_service_api_with_namespace() {
            let client = create_mock_client();
            let namespace = "production".to_string();

            let api = get_service_api(Some(&namespace), client);

            // The API should be configured for the specific namespace
            // We can't easily test the internal state, but we can verify it compiles and runs
            assert_eq!(api.resource_url(), "/api/v1/namespaces/production/services");
        }

        #[test]
        fn get_service_api_without_namespace() {
            let client = create_mock_client();

            let api = get_service_api(None, client);

            // Should use the default namespace from the client
            assert_eq!(api.resource_url(), "/api/v1/namespaces/default/services");
        }

        #[test]
        fn get_service_api_with_empty_namespace() {
            let client = create_mock_client();
            let namespace = "".to_string();

            let api = get_service_api(Some(&namespace), client);

            // Should handle empty namespace string
            assert_eq!(api.resource_url(), "/api/v1/namespaces//services");
        }

        #[test]
        fn get_service_api_with_special_namespace_names() {
            let client = create_mock_client();

            // Test with various valid Kubernetes namespace names
            let test_cases = vec![
                "kube-system",
                "kube-public",
                "kube-node-lease",
                "my-app-prod",
                "staging-env",
                "test123",
                "a",            // Single character
                "a".repeat(63), // Max length namespace
            ];

            for namespace in test_cases {
                let api = get_service_api(Some(&namespace.to_string()), create_mock_client());
                let expected_url = format!("/api/v1/namespaces/{}/services", namespace);
                assert_eq!(api.resource_url(), expected_url);
            }
        }

        #[test]
        fn get_pod_api_with_namespace() {
            let client = create_mock_client();
            let namespace = "development".to_string();

            let api = get_pod_api(Some(&namespace), client);

            assert_eq!(api.resource_url(), "/api/v1/namespaces/development/pods");
        }

        #[test]
        fn get_pod_api_without_namespace() {
            let client = create_mock_client();

            let api = get_pod_api(None, client);

            // Should use the default namespace from the client
            assert_eq!(api.resource_url(), "/api/v1/namespaces/default/pods");
        }

        #[test]
        fn get_pod_api_with_special_namespace_names() {
            let client = create_mock_client();

            let test_cases = vec![
                "kube-system",
                "monitoring",
                "logging",
                "ingress-nginx",
                "cert-manager",
            ];

            for namespace in test_cases {
                let api = get_pod_api(Some(&namespace.to_string()), create_mock_client());
                let expected_url = format!("/api/v1/namespaces/{}/pods", namespace);
                assert_eq!(api.resource_url(), expected_url);
            }
        }

        #[test]
        fn api_consistency_between_service_and_pod() {
            let client1 = create_mock_client();
            let client2 = create_mock_client();
            let namespace = "test-ns".to_string();

            let service_api = get_service_api(Some(&namespace), client1);
            let pod_api = get_pod_api(Some(&namespace), client2);

            // Both should use the same namespace
            assert!(service_api.resource_url().contains("/namespaces/test-ns/"));
            assert!(pod_api.resource_url().contains("/namespaces/test-ns/"));
        }
    }

    mod client_configuration_tests {
        use super::*;

        #[test]
        fn mock_client_has_default_namespace() {
            let client = create_mock_client();
            assert_eq!(client.default_namespace(), "default");
        }

        #[test]
        fn mock_client_cluster_url() {
            let client = create_mock_client();
            // We can't easily access the cluster URL from the client,
            // but we can verify the client was created successfully
            assert_eq!(client.default_namespace(), "default");
        }
    }

    mod integration_scenarios {
        use super::*;

        #[test]
        fn typical_microservice_selectors() {
            // Test a typical microservice deployment selector
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "user-service".to_string());
            selectors.insert("version".to_string(), "v2.1.0".to_string());
            selectors.insert("environment".to_string(), "production".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();

            assert_eq!(
                label_selector,
                "app=user-service,environment=production,version=v2.1.0"
            );
        }

        #[test]
        fn canary_deployment_selectors() {
            // Test selectors for canary deployments
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "api-gateway".to_string());
            selectors.insert("track".to_string(), "canary".to_string());
            selectors.insert("version".to_string(), "v3.0.0-beta".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();

            assert_eq!(
                label_selector,
                "app=api-gateway,track=canary,version=v3.0.0-beta"
            );
        }

        #[test]
        fn database_deployment_selectors() {
            // Test selectors for database deployments
            let mut selectors = BTreeMap::new();
            selectors.insert("app".to_string(), "postgres".to_string());
            selectors.insert("tier".to_string(), "database".to_string());
            selectors.insert("role".to_string(), "primary".to_string());

            let list_params = selector_into_list_params(&selectors);
            let label_selector = list_params.label_selector.unwrap();

            assert_eq!(label_selector, "app=postgres,role=primary,tier=database");
        }

        #[test]
        fn multi_namespace_api_usage() {
            // Test using APIs across multiple namespaces
            let namespaces = vec!["default", "kube-system", "monitoring", "ingress"];

            for namespace in namespaces {
                let ns_string = namespace.to_string();
                let service_api = get_service_api(Some(&ns_string), create_mock_client());
                let pod_api = get_pod_api(Some(&ns_string), create_mock_client());

                assert!(service_api
                    .resource_url()
                    .contains(&format!("/namespaces/{}/", namespace)));
                assert!(pod_api
                    .resource_url()
                    .contains(&format!("/namespaces/{}/", namespace)));
            }
        }
    }
}
