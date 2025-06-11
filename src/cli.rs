use clap::{Args, Parser};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::errors::MyError;

#[derive(Parser, Clone, PartialEq, Debug)]
#[command(author, version, about)]
#[command(long_about = "Multi-service port proxying tool for Kubernetes")]
pub struct CliArgs {
    /// Establish a new port forward - multiple entries can be specified.
    ///
    /// SERVICE:PORT - Binds to localhost (127.0.0.1 and ::1) on PORT and forwards connections to PORT on SERVICE in the default namespace
    /// NAMESPACE/SERVICE:PORT - Binds to localhost (127.0.0.1 and ::1) on PORT and forwards connections to PORT on SERVICE in NAMESPACE
    /// LOCAL_PORT:SERVICE:PORT - Binds to localhost (127.0.0.1 and ::1) on LOCAL_PORT and forwards connections to PORT on SERVICE in the default namespace
    /// LOCAL_ADDRESS:LOCAL_PORT:SERVICE:PORT - Binds to LOCAL_ADDRESS on LOCAL_PORT and forwards connections to PORT on SERVICE in the default namespace
    #[arg(value_name="[[LOCAL_ADDRESS:]LOCAL_PORT:][NAMESPACE/]SERVICE:PORT", required=true, num_args=1.., value_parser=Forward::parse, verbatim_doc_comment)]
    pub forwards: Vec<Forward>,

    /// Kubernetes Context
    #[arg(short, long)]
    pub context: Option<String>,
    /// Default Kubernetes Namespace to match services in
    #[arg(short, long)]
    pub namespace: Option<String>,
    /// Enable compact console output
    #[arg(long)]
    pub compact: bool,

    #[command(flatten)]
    pub control: ControlArgs,
}

#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct ControlArgs {
    /// Don't check the readiness of the pod when selecting which pod to forward to
    #[arg(long)]
    pub ignore_readiness: bool,

    /// Close the connection when the pod goes unready
    #[arg(long)]
    pub close_on_unready: bool,

    /// Chose the pod to connect to randomly instead of the first in the list
    #[arg(long)]
    pub randomise: bool,
}

pub fn parse_args() -> CliArgs {
    CliArgs::parse()
}

#[derive(Debug, PartialEq, Clone)]
pub struct Forward {
    pub service_name: String,
    pub service_port: String,
    pub namespace: Option<String>,
    pub local_address: Option<IpAddr>,
    pub local_port: u16,
}

impl Forward {
    pub fn parse(arg: &str) -> anyhow::Result<Forward> {
        let local_address;
        let local_port_arg;
        let mut service_name;
        let service_port;

        let bits: Vec<&str> = (*arg).rsplitn(4, ':').collect();
        if bits.len() == 4 {
            if bits[3].starts_with('[') && bits[3].ends_with(']') {
                local_address = Some(IpAddr::V6(
                    bits[3][1..(bits[3].len() - 1)].parse::<Ipv6Addr>()?,
                ));
            } else {
                local_address = Some(IpAddr::V4(bits[3].parse::<Ipv4Addr>()?));
            }
            local_port_arg = bits[2].parse::<u16>()?.into();
            service_name = bits[1];
            service_port = bits[0];
        } else if bits.len() == 3 {
            local_address = None;
            local_port_arg = bits[2].parse::<u16>()?.into();
            service_name = bits[1];
            service_port = bits[0];
        } else if bits.len() == 2 {
            local_address = None;
            local_port_arg = Option::<u16>::None;
            service_name = bits[1];
            service_port = bits[0];
        } else {
            return Err(MyError::ArgumentParseError(arg.to_string()).into());
        }

        let local_port = match local_port_arg {
            Some(p) => Ok(p),
            None => service_port.parse(),
        }?;

        let mut namespace = None;
        if service_name.contains('/') {
            let sbits: Vec<&str> = service_name.splitn(2, '/').collect();
            namespace = Some(sbits[0]);
            service_name = sbits[1];
        }

        Ok(Self {
            service_name: service_name.to_owned(),
            service_port: service_port.to_owned(),
            namespace: namespace.map(|s| s.to_owned()),
            local_address,
            local_port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_and_numeric_port() {
        let fwd = Forward::parse("test:1234").unwrap();

        assert_eq!(fwd.namespace, None);
        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port, 1234);
    }

    #[test]
    fn service_name_and_str_port() {
        let fwd = Forward::parse("test:http");

        assert!(fwd.is_err());
    }

    #[test]
    fn local_port_service_name_and_numeric_port() {
        let fwd = Forward::parse("8080:test:1234").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn local_port_service_name_and_str_port() {
        let fwd = Forward::parse("8080:test:http").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "http");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn ipv4_local_port_service_name_and_numeric_port() {
        let fwd = Forward::parse("241.2.124.2:8080:test:1234").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, Some(IpAddr::from([241, 2, 124, 2])));
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn ipv6_local_port_service_name_and_numeric_port() {
        let fwd = Forward::parse("[::1]:8080:test:1234").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]))
        );
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn namespace_service_name_and_numeric_port() {
        let fwd = Forward::parse("namespace/test:1234").unwrap();

        assert_eq!(fwd.namespace, Some("namespace".to_owned()));
        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port, 1234);
    }

    #[test]
    fn complex_namespace_and_ipv4_address() {
        let fwd = Forward::parse("192.168.1.100:9000:my-namespace/my-service:8080").unwrap();

        assert_eq!(fwd.namespace, Some("my-namespace".to_owned()));
        assert_eq!(fwd.service_name, "my-service");
        assert_eq!(fwd.service_port, "8080");
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)))
        );
        assert_eq!(fwd.local_port, 9000);
    }

    #[test]
    fn complex_namespace_and_ipv6_address() {
        let fwd = Forward::parse("[2001:db8::1]:9000:kube-system/dns-service:53").unwrap();

        assert_eq!(fwd.namespace, Some("kube-system".to_owned()));
        assert_eq!(fwd.service_name, "dns-service");
        assert_eq!(fwd.service_port, "53");
        assert_eq!(
            fwd.local_address,
            Some(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)))
        );
        assert_eq!(fwd.local_port, 9000);
    }

    #[test]
    fn namespace_with_local_port_and_named_port() {
        let fwd = Forward::parse("8080:production/api-service:http").unwrap();

        assert_eq!(fwd.namespace, Some("production".to_owned()));
        assert_eq!(fwd.service_name, "api-service");
        assert_eq!(fwd.service_port, "http");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn empty_string_fails() {
        let result = Forward::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn single_component_fails() {
        let result = Forward::parse("service");
        assert!(result.is_err());
    }

    #[test]
    fn too_many_colons_fails() {
        let result = Forward::parse("a:b:c:d:e");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_ipv4_address_fails() {
        let result = Forward::parse("999.999.999.999:8080:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_ipv6_address_fails() {
        let result = Forward::parse("[invalid::ipv6::address]:8080:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_local_port_fails() {
        let result = Forward::parse("99999:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn zero_local_port_allowed() {
        let result = Forward::parse("0:service:80");
        assert!(result.is_ok());
        let fwd = result.unwrap();
        assert_eq!(fwd.local_port, 0);
        assert_eq!(fwd.service_port, "80");
    }

    #[test]
    fn service_name_with_multiple_slashes() {
        let fwd = Forward::parse("namespace/sub/service:8080").unwrap();

        assert_eq!(fwd.namespace, Some("namespace".to_owned()));
        assert_eq!(fwd.service_name, "sub/service");
        assert_eq!(fwd.service_port, "8080");
    }

    #[test]
    fn port_range_max_value() {
        let fwd = Forward::parse("65535:service:80").unwrap();
        assert_eq!(fwd.local_port, 65535);
    }

    #[test]
    fn port_range_exceeds_max_fails() {
        let result = Forward::parse("65536:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn service_port_zero_allowed() {
        let fwd = Forward::parse("8080:service:0").unwrap();
        assert_eq!(fwd.service_port, "0");
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn malformed_ipv6_brackets() {
        let result = Forward::parse("[::1:8080:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn ipv6_without_brackets_fails() {
        let result = Forward::parse("::1:8080:service:80");
        assert!(result.is_err());
    }

    #[test]
    fn localhost_ipv4() {
        let fwd = Forward::parse("127.0.0.1:8080:service:80").unwrap();
        assert_eq!(fwd.local_address, Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn localhost_ipv6() {
        let fwd = Forward::parse("[::1]:8080:service:80").unwrap();
        assert_eq!(fwd.local_address, Some(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn edge_case_high_ports() {
        let fwd = Forward::parse("service:65535").unwrap();
        assert_eq!(fwd.local_port, 65535);
        assert_eq!(fwd.service_port, "65535");
    }

    mod proptest_cases {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn valid_port_numbers_succeed(port in 1u16..=65535) {
                let spec = format!("service:{}", port);
                let fwd = Forward::parse(&spec).unwrap();
                prop_assert_eq!(fwd.local_port, port);
                prop_assert_eq!(fwd.service_port, port.to_string());
            }

            #[test]
            fn service_names_with_alphanumeric_succeed(
                name in "[a-zA-Z][a-zA-Z0-9-]{0,20}",
                port in 1u16..=65535
            ) {
                let spec = format!("{}:{}", name, port);
                let fwd = Forward::parse(&spec).unwrap();
                prop_assert_eq!(fwd.service_name, name);
            }

            #[test]
            fn namespace_names_with_alphanumeric_succeed(
                namespace in "[a-zA-Z][a-zA-Z0-9-]{0,20}",
                service in "[a-zA-Z][a-zA-Z0-9-]{0,20}",
                port in 1u16..=65535
            ) {
                let spec = format!("{}/{}:{}", namespace, service, port);
                let fwd = Forward::parse(&spec).unwrap();
                prop_assert_eq!(fwd.namespace, Some(namespace));
                prop_assert_eq!(fwd.service_name, service);
            }
        }
    }

    mod cli_args_tests {
        use super::*;

        #[test]
        fn control_args_default_values() {
            let args = ControlArgs {
                ignore_readiness: false,
                close_on_unready: false,
                randomise: false,
            };

            assert!(!args.ignore_readiness);
            assert!(!args.close_on_unready);
            assert!(!args.randomise);
        }

        #[test]
        fn control_args_all_flags_set() {
            let args = ControlArgs {
                ignore_readiness: true,
                close_on_unready: true,
                randomise: true,
            };

            assert!(args.ignore_readiness);
            assert!(args.close_on_unready);
            assert!(args.randomise);
        }

        #[test]
        fn cli_args_clone_and_debug() {
            let forward = Forward::parse("test:8080").unwrap();
            let args = CliArgs {
                forwards: vec![forward],
                context: Some("test-context".to_string()),
                namespace: Some("test-namespace".to_string()),
                compact: true,
                control: ControlArgs {
                    ignore_readiness: true,
                    close_on_unready: false,
                    randomise: true,
                },
            };

            let cloned = args.clone();
            assert_eq!(args, cloned);

            let debug_str = format!("{:?}", args);
            assert!(debug_str.contains("test-context"));
            assert!(debug_str.contains("test-namespace"));
        }
    }
}
