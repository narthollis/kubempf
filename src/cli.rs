use clap::{Args, Parser};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::errors::MyError;

#[derive(Parser, Clone, PartialEq, Debug)]
#[command(author, version, about)]
#[command(long_about = "Multi-service port proxying tool for Kubernetes")]
pub struct CliArgs {
    /// Establish a new port forward - multiple entries can be speficied.
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
                local_address = Some(IpAddr::V6(bits[3][1..(bits[3].len() - 1)].parse::<Ipv6Addr>()?));
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
        assert_eq!(fwd.local_address, Some(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1])));
        assert_eq!(fwd.local_port, 8080);
    }

    #[test]
    fn namespace_service_name_and_numeric_port() {
        let fwd = Forward::parse("namespace/test:1234").unwrap();

        assert_eq!(fwd.namespace, Some("namespace".to_owned()));
        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, None);
        assert_eq!(fwd.local_port,  1234);
    }
}
