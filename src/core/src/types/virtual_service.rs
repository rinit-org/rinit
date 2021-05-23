use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Virtual {
    pub providers: Vec<String>,
}
