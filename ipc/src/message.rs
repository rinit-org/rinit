use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    ServiceIsUp(bool, String),
    ServicesStatus(Vec<String>),
    StartServices(Vec<String>),
    StopServices(Vec<String>),
    StartAllServices,
    StopAllServices,
}
