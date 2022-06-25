use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum RequestError {
    #[error("{}", .0)]
    SystemError(String),
    #[error("{}", .0)]
    LogicError(LogicError),
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum LogicError {
    #[error("dependency {dependency} failed to start for service {service}")]
    DependencyFailedToStart { service: String, dependency: String },
    #[error("service {service} dependendents {dependents:?} are still running")]
    DependentsStillRunning {
        service: String,
        dependents: Vec<String>,
    },
    #[error("service {service} failed to start")]
    ServiceFailedToStart { service: String },
    #[error("service {service} does not exists")]
    ServiceNotFound { service: String },
}
