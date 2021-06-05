use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Bundle {
    pub contents: Vec<String>,
}
