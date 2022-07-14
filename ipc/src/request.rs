use rinit_service::{
    service_state::ServiceState,
    types::RunLevel,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    ServiceIsUp(String, bool),
    ServicesStatus(),
    ServiceStatus(String),
    StartService { service: String, runlevel: RunLevel },
    StopService { service: String, runlevel: RunLevel },
    StartAllServices(RunLevel),
    StopAllServices(RunLevel),
    ReloadGraph,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Reply {
    ServicesStates(Vec<(String, ServiceState)>),
    Result(Option<String>),
    Empty,
}
