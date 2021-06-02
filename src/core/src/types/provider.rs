use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum Provider {
    Empty,
    Single(String),
    Multiple(Vec<String>),
}
