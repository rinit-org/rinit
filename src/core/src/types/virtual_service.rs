use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Virtual {
    pub providers: Vec<String>,
}
