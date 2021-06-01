use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ScriptConfig {
    contents: HashMap<String, String>,
}

impl ScriptConfig {
    pub fn new() -> ScriptConfig {
        ScriptConfig {
            contents: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        key: &str,
        value: String,
    ) -> Result<(), String> {
        match self.contents.get(key) {
            Some(_) => Err(format!("'{}' has already been set", key)),
            None => {
                self.contents.insert(String::from(key), value);
                Ok(())
            }
        }
    }

    pub fn get(
        &self,
        key: &str,
    ) -> Result<&str, String> {
        match self.contents.get(key) {
            Some(value) => Ok(value),
            None => Err(format!("'{}' has not been set", key)),
        }
    }
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let mut conf = ScriptConfig::new();
        conf.add("key1", String::from("value1")).unwrap();
    }

    #[test]
    fn add_multiple_times() {
        let mut conf = ScriptConfig::new();
        conf.add("key1", String::from("value1")).unwrap();
        let res = conf.add("key1", String::from("value2"));
        let expected = Err(String::from("'key1' has already been set"));
        assert_eq!(res, expected);
    }

    #[test]
    fn get_non_existant_value() {
        let conf = ScriptConfig::new();
        let res = conf.get("key1");
        let expected = Err(String::from("'key1' has not been set"));
        assert_eq!(res, expected);
    }

    #[test]
    fn get_value() {
        let mut conf = ScriptConfig::new();
        conf.add("key1", String::from("value1")).unwrap();
        let value = conf.get("key1").unwrap();
        assert_eq!(value, String::from("value1"));
    }
}
