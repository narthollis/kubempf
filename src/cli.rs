use clap::{arg, Command};
use kube::{api::ListParams, Api};
use k8s_openapi::{apimachinery::pkg::util::intstr::IntOrString, api::core::v1::Service};
use std::{net::SocketAddr, collections::BTreeMap};

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
                .help("Kubernetes Namespace"),
        )
        .arg(
            arg!([FORWARD])
                .id("forwards")
                .num_args(1..)
                .required(true)
                .help("[[LOCAL_ADDRESS:]LOCAL_PORT:]service:port"),
        )
}


#[derive(Debug)]
pub struct Forward {
    pub service_name: String,
    pub service_port: String,
    pub pod_list_params: ListParams,
    pub pod_port: IntOrString,
    pub local_address: SocketAddr,
}

impl Forward {
    pub async fn parse(api: &Api<Service>, arg: &str) -> anyhow::Result<Forward> {
        let local_address = [127, 0, 0, 1];
        let local_port_arg;
        let service_name;
        let service_port_arg;

        let bits: Vec<&str> = (*arg).split(':').collect();
        if bits.len() == 4 {
            // todo parse local address
            local_port_arg = bits[1].parse::<u16>()?.into();
            service_name = bits[2];
            service_port_arg = bits[3];
        } else if bits.len() == 3 {
            local_port_arg = bits[0].parse::<u16>()?.into();
            service_name = bits[1];
            service_port_arg = bits[2];
        } else if bits.len() == 2 {
            local_port_arg = Option::<u16>::None;
            service_name = bits[0];
            service_port_arg = bits[1];
        } else {
            return Err(MyError::ArgumentParseError(arg.to_string()).into());
        }

        let service = api.get(service_name).await?;
        let service_spec = service
            .spec
            .ok_or(MyError::ServiceNotFound(service_name.to_string()))?;
        let selector = service_spec
            .selector
            .ok_or(MyError::ServiceMissingSelectors(service_name.to_string()))?;

        let pod_port: IntOrString = match service_port_arg.parse::<i32>() {
            Ok(p) => Ok(IntOrString::Int(p)),
            Err(_) => service_spec
                .ports
                .and_then(|pl| {
                    pl.into_iter()
                        .find(|p| p.name == Some(service_port_arg.to_string()))
                })
                .map(|p| p.target_port.unwrap_or(IntOrString::Int(p.port)))
                .ok_or(MyError::MissingNamedPort(
                    service_port_arg.to_string(),
                    service_name.to_string(),
                )),
        }?;

        let local_port = match local_port_arg {
            Some(p) => Ok(p),
            None => match pod_port {
                IntOrString::Int(p) => (p).try_into().map_err(|_| {
                    MyError::UnableToConvertServicePort(p.to_string(), service_name.to_string())
                }),
                IntOrString::String(_) => Err(MyError::UnableToInferLocalPort(
                    service_port_arg.to_string(),
                    service_name.to_string(),
                )),
            },
        }?;

        Ok(Self {
            service_name: service_name.to_string(),
            service_port: service_port_arg.to_string(),
            pod_list_params: selector_into_list_params(&selector),
            pod_port,
            local_address: SocketAddr::from((local_address, local_port)),
        })
    }
}

fn selector_into_list_params(selectors: &BTreeMap<String, String>) -> ListParams {
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

    ListParams::default().labels(&labels)
}