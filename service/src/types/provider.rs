use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Provider {
    Empty,
    Single(String),
    Multiple(Vec<String>),
}
