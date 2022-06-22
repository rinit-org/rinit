use rinit_service::service_state::ServiceState;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    ServiceIsUp(bool, String),
    ServicesStatus(Vec<String>),
    StartServices(Vec<String>),
    StopServices(Vec<String>),
    StartAllServices,
    StopAllServices,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Reply {
    ServicesStates(Vec<(String, ServiceState)>),
    Result(Option<String>),
    Empty,
}
