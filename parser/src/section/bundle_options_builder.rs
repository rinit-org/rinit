use std::{
    collections::HashMap,
    str::FromStr,
};

use rinit_service::types::{
    BundleOptions,
    RunLevel,
};
use snafu::{
    ResultExt,
    Snafu,
};

use super::SectionBuilder;

#[derive(Snafu, Debug)]
pub enum BundleOptionsBuilderError {
    #[snafu(display("LOL"))]
    EmptyContents,
    #[snafu(display("runlevel value is not correct"))]
    RunLevelParseError,
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
        values: &mut HashMap<&'static str, String>,
        array_values: &mut HashMap<&'static str, Vec<String>>,
        _code_values: &mut HashMap<&'static str, String>,
    ) {
        let contents = array_values.remove("contents");
        let runlevel = values
            .remove("runlevel")
            .map_or(Ok(RunLevel::default()), |s| RunLevel::from_str(&s))
            .with_context(|_| RunLevelParseSnafu);
        self.bundle_options = Some(
            contents.map_or(Err(BundleOptionsBuilderError::EmptyContents), |contents| {
                runlevel.map(|runlevel| BundleOptions { contents, runlevel })
            }),
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
