use thiserror::Error;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("unable to parse argument {0}")]
    ArgumentParseError(String),
    #[error("unable to find named port {0} on service {1}")]
    MissingNamedPort(String, String),
    #[error("unable to infer local port from named service port {0} for service {1}")]
    UnableToInferLocalPort(String, String),
    #[error("unable to convert service port {0} to u16 for service {1}")]
    UnableToConvertServicePort(String, String),
    #[error("service {0} not found or invalid")]
    ServiceNotFound(String),
    #[error("service {0} not compatiable as it is is missing selectors")]
    ServiceMissingSelectors(String),
    #[error("no matching ready pods")]
    MatchingReadyPodNotFound(),
    #[error("service is referencing `{0:#?}` in pod - but this does not exist on the pod")]
    CouldNotFindPort(IntOrString),
}