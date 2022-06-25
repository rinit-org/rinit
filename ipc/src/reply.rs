use rinit_service::service_state::ServiceState;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum Reply {
    ServicesStates(Vec<(String, ServiceState)>),
    ServiceState(String, ServiceState),
    Success(bool),
    Empty,
}
