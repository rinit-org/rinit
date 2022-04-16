use kansei_core::service_state::ServiceState;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum Reply {
    ServicesStates(Vec<(String, ServiceState)>),
    Result(Option<String>),
}
