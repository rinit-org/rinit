use std::collections::HashMap;

use kansei_core::types::ServiceOptions;
use snafu::Snafu;

use super::SectionBuilder;

#[derive(Snafu, Debug)]
pub struct ServiceOptionsBuilderError {}

pub struct ServiceOptionsBuilder {
    pub options: Option<Result<ServiceOptions, ServiceOptionsBuilderError>>,
}

type Result<T, E = ServiceOptionsBuilderError> = std::result::Result<T, E>;

impl ServiceOptionsBuilder {
    pub fn new() -> Self {
        ServiceOptionsBuilder { options: None }
    }
}

impl SectionBuilder for ServiceOptionsBuilder {
    fn build(
        &mut self,
        _values: &mut HashMap<&'static str, String>,
        array_values: &mut HashMap<&'static str, Vec<String>>,
        _code_values: &mut HashMap<&'static str, String>,
    ) {
        let dependencies = array_values.remove("dependencies").unwrap_or_default();
        let requires = array_values.remove("requires").unwrap_or_default();
        let requires_one = array_values.remove("requires-one").unwrap_or_default();
        self.options = Some(Ok(ServiceOptions {
            dependencies,
            requires,
            requires_one,
        }));
    }

    fn section_name(&self) -> &'static str {
        "options"
    }

    fn get_fields(&self) -> &'static [&'static str] {
        &[]
    }

    fn get_array_fields(&self) -> &'static [&'static str] {
        &["dependencies", "requires", "requires-one"]
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
        let mut builder = ServiceOptionsBuilder::new();
        assert!(
            builder
                .parse_until_next_section(&[
                    "dependencies = [ foo ]".to_string(),
                    "requires = [ bar ]".to_string(),
                    "requires-one = [ foobar ]".to_string()
                ])
                .unwrap()
                .is_empty()
        );

        let options = builder.options.unwrap().unwrap();
        assert_eq!(options.dependencies, vec!["foo".to_string()]);
        assert_eq!(options.requires, vec!["bar".to_string()]);
        assert_eq!(options.requires_one, vec!["foobar".to_string()]);
    }
}
