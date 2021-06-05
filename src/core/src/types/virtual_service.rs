use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Virtual {
    pub name: String,
    pub providers: Vec<String>,
}
