use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Bundle {
    pub name: String,
    pub contents: Vec<String>,
}
