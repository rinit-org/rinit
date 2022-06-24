use std::collections::HashMap;

use rinit_service::types::ServiceOptions;
use snafu::{
    ResultExt,
    Snafu,
};

use super::SectionBuilder;

#[derive(Snafu, Debug)]
pub enum ServiceOptionsBuilderError {
    #[snafu(display("{} must be either 'yes' or 'no'", key))]
    InvalidBoolean { key: String },
}

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
        values: &mut HashMap<&'static str, String>,
        array_values: &mut HashMap<&'static str, Vec<String>>,
        _code_values: &mut HashMap<&'static str, String>,
    ) {
        let dependencies = array_values.remove("dependencies").unwrap_or_default();
        let requires = array_values.remove("requires").unwrap_or_default();
        let requires_one = array_values.remove("requires-one").unwrap_or_default();
        // TODO: return error when auto-start is != yes and != no
        let autostart = values
            .remove("autostart")
            .map_or(Ok(true), |autostart| {
                match autostart.as_str() {
                    "yes" => Ok(true),
                    "no" => Ok(false),
                    _ => Err(snafu::NoneError),
                }
            })
            .with_context(|_| {
                InvalidBooleanSnafu {
                    key: "autostart".to_string(),
                }
            });
        self.options = Some(autostart.map(|autostart| -> ServiceOptions {
            ServiceOptions {
                dependencies,
                requires,
                requires_one,
                autostart,
            }
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
                    "dependencies = [ foo ]",
                    "requires = [ bar ]",
                    "requires-one = [ foobar ]"
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
