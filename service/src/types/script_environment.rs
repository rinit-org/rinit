use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ScriptEnvironment {
    #[serde(default)]
    pub contents: Vec<(String, String)>,
}

impl ScriptEnvironment {
    pub fn new() -> ScriptEnvironment {
        ScriptEnvironment {
            contents: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        key: &str,
        value: String,
    ) {
        self.contents.push((String::from(key), value));
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

impl Default for ScriptEnvironment {
    fn default() -> Self {
        Self::new()
    }
}
