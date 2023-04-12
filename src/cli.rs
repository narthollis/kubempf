use clap::{arg, Command};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::errors::MyError;

pub fn cli() -> Command {
    Command::new("kubempf")
        .about("Multi-service port proxying tool for Kubernetes")
        .arg_required_else_help(true)
        .allow_external_subcommands(false)
        .arg(
            arg!(-c - -context[CONTEXT])
                .required(false)
                .require_equals(false)
                .help("Kubernetes Context"),
        )
        .arg(
            arg!(-n - -namespace[NAMESPACE])
                .required(false)
                .require_equals(false)
                .help("Default Kubernetes Namespace to match services in"),
        )
        .arg(
            arg!(--compact)
                .required(false)
                .action(clap::ArgAction::SetTrue)
                .help("Enable compact console output"),
        )
        .arg(
            arg!([FORWARD])
                .id("forwards")
                .num_args(1..)
                .required(true)
                .help("[[LOCAL_ADDRESS:]LOCAL_PORT:][namespace/]service:port"),
        )
}

#[derive(Debug)]
pub struct Forward<'a> {
    pub service_name: &'a str,
    pub service_port: &'a str,
    pub namespace: Option<&'a str>,
    pub local_address: SocketAddr,
}

impl<'a> Forward<'a> {
    pub fn parse(arg: &'a str) -> anyhow::Result<Forward<'a>> {
        let local_address;
        let local_port_arg;
        let mut service_name;
        let service_port;

        let bits: Vec<&str> = (*arg).rsplitn(4, ':').collect();
        if bits.len() == 4 {
            if bits[3].starts_with('[') && bits[3].ends_with(']') {
                local_address = IpAddr::V6(bits[3][1..(bits[3].len() - 1)].parse::<Ipv6Addr>()?);
            } else {
                local_address = IpAddr::V4(bits[3].parse::<Ipv4Addr>()?);
            }
            local_port_arg = bits[2].parse::<u16>()?.into();
            service_name = bits[1];
            service_port = bits[0];
        } else if bits.len() == 3 {
            local_address = IpAddr::from([127, 0, 0, 1]);
            local_port_arg = bits[2].parse::<u16>()?.into();
            service_name = bits[1];
            service_port = bits[0];
        } else if bits.len() == 2 {
            local_address = IpAddr::from([127, 0, 0, 1]);
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
            service_name,
            service_port,
            namespace,
            local_address: SocketAddr::from((local_address, local_port)),
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
        assert_eq!(fwd.local_address, SocketAddr::from(([127, 0, 0, 1], 1234)));
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
        assert_eq!(fwd.local_address, SocketAddr::from(([127, 0, 0, 1], 8080)));
    }

    #[test]
    fn local_port_service_name_and_str_port() {
        let fwd = Forward::parse("8080:test:http").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "http");
        assert_eq!(fwd.local_address, SocketAddr::from(([127, 0, 0, 1], 8080)));
    }

    #[test]
    fn ipv4_local_port_service_name_and_numeric_port() {
        let fwd = Forward::parse("241.2.124.2:8080:test:1234").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(
            fwd.local_address,
            SocketAddr::from(([241, 2, 124, 2], 8080))
        );
    }

    #[test]
    fn ipv6_local_port_service_name_and_numeric_port() {
        let fwd = Forward::parse("[::1]:8080:test:1234").unwrap();

        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(
            fwd.local_address,
            SocketAddr::from((IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]), 8080))
        );
    }

    #[test]
    fn namespace_service_name_and_numeric_port() {
        let fwd = Forward::parse("namespace/test:1234").unwrap();

        assert_eq!(fwd.namespace, Some("namespace"));
        assert_eq!(fwd.service_name, "test");
        assert_eq!(fwd.service_port, "1234");
        assert_eq!(fwd.local_address, SocketAddr::from(([127, 0, 0, 1], 1234)));
    }
}
