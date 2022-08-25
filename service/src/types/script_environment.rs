use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct ScriptEnvironment {
    #[serde(default)]
    contents: HashMap<String, String>,
}

impl ScriptEnvironment {
    pub fn new() -> ScriptEnvironment {
        ScriptEnvironment {
            contents: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        key: &str,
        value: String,
    ) {
        self.contents.insert(String::from(key), value);
    }

    pub fn get(
        &self,
        key: &str,
    ) -> Option<&str> {
        match self.contents.get(key) {
            Some(value) => Some(value),
            None => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let mut conf = ScriptEnvironment::new();
        conf.add("key1", String::from("value1"));
        assert_eq!(conf.get("key1").unwrap(), "value1");
    }

    #[test]
    fn add_multiple_times() {
        let mut conf = ScriptEnvironment::new();
        conf.add("key1", String::from("value1"));
        conf.add("key1", String::from("value2"));
        assert_eq!(conf.get("key1").unwrap(), "value2");
    }

    #[test]
    fn get_non_existant_value() {
        let conf = ScriptEnvironment::new();
        assert!(conf.get("key1").is_none());
    }
}
