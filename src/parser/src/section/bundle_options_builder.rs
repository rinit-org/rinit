use std::collections::HashMap;

use kansei_core::types::BundleOptions;
use snafu::Snafu;

use super::SectionBuilder;

#[derive(Snafu, Debug)]
pub enum BundleOptionsBuilderError {
    #[snafu(display("LOL"))]
    EmptyContents,
}

pub struct BundleOptionsBuilder {
    pub bundle_options: Option<Result<BundleOptions, BundleOptionsBuilderError>>,
}

type Result<T, E = BundleOptionsBuilderError> = std::result::Result<T, E>;

impl BundleOptionsBuilder {
    pub fn new() -> Self {
        BundleOptionsBuilder {
            bundle_options: None,
        }
    }
}

impl SectionBuilder for BundleOptionsBuilder {
    fn build(
        &mut self,
        _values: &mut HashMap<&'static str, String>,
        array_values: &mut HashMap<&'static str, Vec<String>>,
        _code_values: &mut HashMap<&'static str, String>,
    ) {
        let contents = array_values.remove("contents");
        self.bundle_options = Some(
            if let Some(contents) = contents {
                Ok(BundleOptions { contents })
            } else {
                Err(BundleOptionsBuilderError::EmptyContents {})
            },
        );
    }

    fn section_name(&self) -> &'static str {
        "options"
    }

    fn get_fields(&self) -> &'static [&'static str] {
        &[]
    }

    fn get_array_fields(&self) -> &'static [&'static str] {
        &["contents"]
    }

    fn get_code_fields(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::section::SectionBuilderError;

    #[test]
    fn parse_section() {
        let mut builder = BundleOptionsBuilder::new();
        assert!(
            builder
                .parse_until_next_section(&["contents = [ foo ]"])
                .unwrap()
                .is_empty()
        );

        let bundle_options = builder.bundle_options.unwrap().unwrap();
        assert_eq!(bundle_options.contents, vec!["foo".to_string()]);
    }

    #[test]
    fn parse_section_invalid_field() {
        let mut builder = BundleOptionsBuilder::new();
        assert_eq!(
            builder
                .parse_until_next_section(&["foo = [ bar ]"])
                .unwrap_err(),
            SectionBuilderError::InvalidField {
                field: "foo".to_string()
            }
        );
    }
}
