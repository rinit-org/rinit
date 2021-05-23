use serde::{
    Deserialize,
    Serialize,
};

use super::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Service {
    Bundle(Bundle),
    Longrun(Longrun),
    Oneshot(Oneshot),
    Virtual(Virtual),
}
