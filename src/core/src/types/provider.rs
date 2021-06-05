use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Provider {
    Empty,
    Single(String),
    Multiple(Vec<String>),
}
