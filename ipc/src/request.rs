use rinit_service::service_state::ServiceState;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    ServiceIsUp(String, bool),
    ServicesStatus(),
    ServiceStatus(String),
    StartService(String),
    StopService(String),
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
