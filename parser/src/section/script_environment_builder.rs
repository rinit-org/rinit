use std::collections::HashMap;

use rinit_service::types::ScriptEnvironment;
use snafu::{
    ResultExt,
    Snafu,
};
use snailquote::{
    unescape,
    UnescapeError,
};

use super::{
    SectionBuilder,
    SectionBuilderError,
};
use crate::parse_section::parse_section;

#[derive(Snafu, Debug)]
pub enum ScriptEnvironmentBuilderError {
    #[snafu(display("{source}"))]
    UnescapeError { source: UnescapeError },
}

pub struct ScriptEnvironmentBuilder {
    pub environment: Option<Result<ScriptEnvironment, ScriptEnvironmentBuilderError>>,
}

type Result<T, E = ScriptEnvironmentBuilderError> = std::result::Result<T, E>;

impl ScriptEnvironmentBuilder {
    pub fn new() -> Self {
        Self { environment: None }
    }
}

impl SectionBuilder for ScriptEnvironmentBuilder {
    fn parse_until_next_section<'a>(
        &mut self,
        lines: &'a [&'a str],
    ) -> Result<&'a [&'a str], SectionBuilderError> {
        let mut next_section: &'a [&str] = &[];
        let mut env: Result<ScriptEnvironment, ScriptEnvironmentBuilderError> =
            Ok(ScriptEnvironment::new());
        for (index, line) in lines.iter().enumerate() {
            if parse_section(line).is_some() {
                next_section = &lines[index..];
                break;
            } else if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                env = unescape(value)
                    .with_context(|_| UnescapeSnafu {})
                    .and_then(|value| {
                        env.as_mut().unwrap().add(key, value);
                        env
                    });
            }
        }
        self.environment = Some(env);
        Ok(next_section)
    }

    fn build(
        &mut self,
        _values: &mut HashMap<&'static str, String>,
        _array_values: &mut HashMap<&'static str, Vec<String>>,
        _code_values: &mut HashMap<&'static str, String>,
    ) {
        // We are using a custom implementation of parse_until_next_section instead of
        // the default one defined in the trait
        unreachable!();
    }

    fn section_name(&self) -> &'static str {
        "config"
    }

    fn get_fields(&self) -> &'static [&'static str] {
        &[]
    }

    fn get_array_fields(&self) -> &'static [&'static str] {
        &[]
    }

    fn get_code_fields(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_section() {
        let mut builder = ScriptEnvironmentBuilder::new();
        assert!(
            builder
                .parse_until_next_section(&["MYENV = \"MYVAL\""])
                .unwrap()
                .is_empty()
        );

        let env = builder.environment.unwrap().unwrap();
        assert_eq!(env.get("MYENV").unwrap(), "MYVAL");
    }
}
