use serde::{
    Deserialize,
    Serialize,
};

use super::*;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Service {
    Bundle(Bundle),
    Longrun(Longrun),
    Oneshot(Oneshot),
    Virtual(Virtual),
}

impl Service {
    pub fn name(&self) -> &str {
        match &self {
            Self::Bundle(bundle) => &bundle.name,
            Self::Longrun(longrun) => &longrun.name,
            Self::Oneshot(oneshot) => &oneshot.name,
            Self::Virtual(virtual_service) => &virtual_service.name,
        }
    }

    pub fn dependencies(&self) -> &[String] {
        match &self {
            Self::Bundle(bundle) => &bundle.contents,
            Self::Longrun(longrun) => &longrun.options.dependencies,
            Self::Oneshot(oneshot) => &oneshot.options.dependencies,
            // TODO: What should be done here?
            Self::Virtual(_virtual_service) => &[],
        }
    }
}
