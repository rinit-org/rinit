use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Bundle {
    pub contents: Vec<String>,
}
