use rinit_service::{
    service_state::{
        IdleServiceState,
        ServiceState,
    },
    types::RunLevel,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    UpdateServiceStatus(String, IdleServiceState),
    ServicesStatus,
    ServiceStatus(String),
    StartService { service: String, runlevel: RunLevel },
    StopService { service: String, runlevel: RunLevel },
    StartAllServices,
    StopAllServices,
    ReloadGraph,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Reply {
    ServicesStates(Vec<(String, ServiceState)>),
    Result(Option<String>),
    Empty,
}
