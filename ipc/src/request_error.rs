use serde::{
    Deserialize,
    Serialize,
};
use snafu::Snafu;

#[derive(Snafu, Debug, Serialize, Deserialize)]
pub enum RequestError {
    #[snafu(display("{err}"))]
    SystemError { err: String },
    #[snafu(display("{err}"))]
    LogicError { err: LogicError },
}

#[derive(Snafu, Debug, Serialize, Deserialize)]
#[snafu(visibility(pub))]
pub enum LogicError {
    #[snafu(display("dependency {dependency} failed to start for service {service}"))]
    DependencyFailedToStart { service: String, dependency: String },
    #[snafu(display("service {service} dependendents {dependents:?} are still running"))]
    DependentsStillRunning {
        service: String,
        dependents: Vec<String>,
    },
    #[snafu(display("dependency graph not found in path {path}"))]
    DependencyGraphNotFound { path: String },
    #[snafu(display("service {service} failed to start"))]
    ServiceFailedToStart { service: String },
    #[snafu(display("service {service} does not exists"))]
    ServiceNotFound { service: String },
}
