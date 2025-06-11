//! Comprehensive CLI argument parsing tests for kubempf

#[cfg(test)]
mod tests {
    use crate::cli::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn cli_args_with_single_forward() {
        let args = CliArgs {
            forwards: vec![Forward::parse("service:8080").unwrap()],
            context: None,
            namespace: None,
            compact: false,
            control: ControlArgs {
                ignore_readiness: false,
                close_on_unready: false,
                randomise: false,
            },
        };

        assert_eq!(args.forwards.len(), 1);
        assert_eq!(args.forwards[0].service_name, "service");
        assert_eq!(args.forwards[0].service_port, "8080");
    }

    #[test]
    fn cli_args_with_multiple_forwards() {
        let forwards = vec![
            Forward::parse("service1:8080").unwrap(),
            Forward::parse("namespace/service2:9090").unwrap(),
            Forward::parse("127.0.0.1:3000:service3:http").unwrap(),
        ];

        let args = CliArgs {
            forwards,
            context: Some("production".to_string()),
            namespace: Some("default".to_string()),
            compact: true,
            control: ControlArgs {
                ignore_readiness: true,
                close_on_unready: true,
                randomise: true,
            },
        };

        assert_eq!(args.forwards.len(), 3);
        assert_eq!(args.context, Some("production".to_string()));
        assert_eq!(args.namespace, Some("default".to_string()));
        assert!(args.compact);
        assert!(args.control.ignore_readiness);
        assert!(args.control.close_on_unready);
        assert!(args.control.randomise);
    }

    #[test]
    fn forward_validation_edge_cases() {
        // Test minimum port
        let fwd = Forward::parse("service:1").unwrap();
        assert_eq!(fwd.local_port, 1);

        // Test maximum port
        let fwd = Forward::parse("service:65535").unwrap();
        assert_eq!(fwd.local_port, 65535);

        // Test named port
        let fwd = Forward::parse("8080:service:https").unwrap();
        assert_eq!(fwd.service_port, "https");
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn forward_error_cases() {
        // Invalid port range
        assert!(Forward::parse("service:65536").is_err());

        // Malformed input
        assert!(Forward::parse("").is_err());
        assert!(Forward::parse("service").is_err());
        assert!(Forward::parse("a:b:c:d:e").is_err());

        // Invalid IP addresses
        assert!(Forward::parse("999.999.999.999:8080:service:80").is_err());
        assert!(Forward::parse("[invalid::ipv6]:8080:service:80").is_err());
    }

    #[test]
    fn forward_namespace_parsing() {
        // Simple namespace
        let fwd = Forward::parse("production/api:8080").unwrap();
        assert_eq!(fwd.namespace, Some("production".to_string()));
        assert_eq!(fwd.service_name, "api");

        // Namespace with hyphens and numbers
        let fwd = Forward::parse("kube-system123/dns-service:53").unwrap();
        assert_eq!(fwd.namespace, Some("kube-system123".to_string()));
        assert_eq!(fwd.service_name, "dns-service");

        // Service name with slash (should be treated as part of service name)
        let fwd = Forward::parse("ns/service/path:80").unwrap();
        assert_eq!(fwd.namespace, Some("ns".to_string()));
        assert_eq!(fwd.service_name, "service/path");
    }

    #[test]
    fn forward_ip_address_parsing() {
        // IPv4 addresses
        let fwd = Forward::parse("192.168.1.1:8080:service:80").unwrap();
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
        );

        let fwd = Forward::parse("10.0.0.0:8080:service:80").unwrap();
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)))
        );

        // IPv6 addresses
        let fwd = Forward::parse("[::1]:8080:service:80").unwrap();
        assert_eq!(fwd.local_address, Some(IpAddr::V6(Ipv6Addr::LOCALHOST)));

        let fwd = Forward::parse("[2001:db8::1]:8080:service:80").unwrap();
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)))
        );

        // Full IPv6 address
        let fwd =
            Forward::parse("[2001:0db8:85a3:0000:0000:8a2e:0370:7334]:8080:service:80").unwrap();
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V6(Ipv6Addr::new(
                0x2001, 0xdb8, 0x85a3, 0, 0, 0x8a2e, 0x370, 0x7334
            )))
        );
    }

    #[test]
    fn control_args_combinations() {
        // Test each flag individually
        let args1 = ControlArgs {
            ignore_readiness: true,
            close_on_unready: false,
            randomise: false,
        };
        assert!(args1.ignore_readiness && !args1.close_on_unready && !args1.randomise);

        let args2 = ControlArgs {
            ignore_readiness: false,
            close_on_unready: true,
            randomise: false,
        };
        assert!(!args2.ignore_readiness && args2.close_on_unready && !args2.randomise);

        let args3 = ControlArgs {
            ignore_readiness: false,
            close_on_unready: false,
            randomise: true,
        };
        assert!(!args3.ignore_readiness && !args3.close_on_unready && args3.randomise);

        // Test conflicting flags (both should be allowed)
        let args4 = ControlArgs {
            ignore_readiness: true,
            close_on_unready: true,
            randomise: true,
        };
        assert!(args4.ignore_readiness && args4.close_on_unready && args4.randomise);
    }

    #[test]
    fn forward_struct_equality() {
        let fwd1 = Forward::parse("service:8080").unwrap();
        let fwd2 = Forward::parse("service:8080").unwrap();
        assert_eq!(fwd1, fwd2);

        let fwd3 = Forward::parse("127.0.0.1:9000:service:8080").unwrap();
        let fwd4 = Forward::parse("127.0.0.1:9000:service:8080").unwrap();
        assert_eq!(fwd3, fwd4);

        // Different forwards should not be equal
        let fwd5 = Forward::parse("service:8080").unwrap();
        let fwd6 = Forward::parse("service:9090").unwrap();
        assert_ne!(fwd5, fwd6);
    }

    #[test]
    fn forward_clone_and_debug() {
        let original = Forward::parse("production/api:8080").unwrap();
        let cloned = original.clone();

        assert_eq!(original, cloned);

        let debug_str = format!("{:?}", original);
        assert!(debug_str.contains("production"));
        assert!(debug_str.contains("api"));
        assert!(debug_str.contains("8080"));
    }

    #[test]
    fn special_service_names() {
        // Service names with numbers
        let fwd = Forward::parse("api-v2:8080").unwrap();
        assert_eq!(fwd.service_name, "api-v2");

        // Service names with underscores
        let fwd = Forward::parse("user_service:8080").unwrap();
        assert_eq!(fwd.service_name, "user_service");

        // Very long service names
        let long_name = "a".repeat(63); // Kubernetes max length
        let spec = format!("{}:8080", long_name);
        let fwd = Forward::parse(&spec).unwrap();
        assert_eq!(fwd.service_name, long_name);
    }

    #[test]
    fn port_zero_handling() {
        // Service port 0 should be allowed (dynamic port allocation)
        let fwd = Forward::parse("8080:service:0").unwrap();
        assert_eq!(fwd.service_port, "0");
        assert_eq!(fwd.local_port, 8080);

        // Port 0 is actually allowed for local ports too in the current implementation
        let fwd = Forward::parse("0:service:8080").unwrap();
        assert_eq!(fwd.local_port, 0);
        assert_eq!(fwd.service_port, "8080");

        // But service:0 with no local port specified should work too
        let fwd = Forward::parse("service:0").unwrap();
        assert_eq!(fwd.local_port, 0);
        assert_eq!(fwd.service_port, "0");
    }

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn any_valid_port_should_parse(port in 1u16..=65535) {
                let spec = format!("service:{}", port);
                let result = Forward::parse(&spec);
                prop_assert!(result.is_ok());
                let fwd = result.unwrap();
                prop_assert_eq!(fwd.local_port, port);
                prop_assert_eq!(fwd.service_port, port.to_string());
            }

            #[test]
            fn valid_ipv4_addresses_should_parse(
                a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
                port in 1u16..=65535
            ) {
                let spec = format!("{}.{}.{}.{}:8080:service:{}", a, b, c, d, port);
                let result = Forward::parse(&spec);
                prop_assert!(result.is_ok());
                let fwd = result.unwrap();
                prop_assert_eq!(fwd.local_address, Some(IpAddr::V4(Ipv4Addr::new(a, b, c, d))));
            }

            #[test]
            fn kubernetes_compliant_names_should_parse(
                name in "[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?",
                port in 1u16..=65535
            ) {
                let spec = format!("{}:{}", name, port);
                let result = Forward::parse(&spec);
                prop_assert!(result.is_ok());
                let fwd = result.unwrap();
                prop_assert_eq!(fwd.service_name, name);
            }
        }
    }
}
