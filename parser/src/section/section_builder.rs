use std::{
    any::Any,
    collections::HashMap,
};

use snafu::{
    ensure,
    ResultExt,
    Snafu,
};

use crate::{
    code_parser::CodeParser,
    is_empty_line::is_empty_line,
    parse_section::parse_section,
    ArrayParser,
};

#[derive(Snafu, Debug, PartialEq, Eq)]
pub enum SectionBuilderError {
    #[snafu(display("encountered an error while parsing the field {}", field))]
    ArrayParserError {
        field: String,
        source: crate::ArrayParserError,
    },
    #[snafu(display("the following elements are duplicated: {:?}", duplicates))]
    ArrayWithDuplicates { duplicates: Vec<String> },
    #[snafu(display("field {} has not been closed", field))]
    ArrayNotClosed { field: String },
    #[snafu(display("error while parsing code"))]
    CodeParserError {
        source: crate::code_parser::CodeParserError,
    },
    #[snafu(display("{} field has already been set", field))]
    DuplicateField { field: String },
    #[snafu(display("{} is not a valid field", field))]
    InvalidField { field: String },
}

type Result<T, E = SectionBuilderError> = std::result::Result<T, E>;

fn add_field_value<T>(
    key: &str,
    value: T,
    values: &mut HashMap<&'static str, T>,
    fields: &'static [&'static str],
) -> Result<()> {
    ensure!(
        !values.contains_key(key),
        DuplicateFieldSnafu {
            field: key.to_owned()
        }
    );

    let map_key = fields.iter().find(|s| *s == &key);
    ensure!(
        map_key.is_some(),
        InvalidFieldSnafu {
            field: key.to_owned()
        }
    );

    values.insert(map_key.unwrap(), value);
    Ok(())
}

pub trait SectionBuilder: Any {
    fn section_name(&self) -> &'static str;
    fn get_fields(&self) -> &'static [&'static str];
    fn get_array_fields(&self) -> &'static [&'static str];
    fn get_code_fields(&self) -> &'static [&'static str];

    fn build(
        &mut self,
        values: &mut HashMap<&'static str, String>,
        array_values: &mut HashMap<&'static str, Vec<String>>,
        code_values: &mut HashMap<&'static str, String>,
    );

    // Parse line per line and fill the three structures values, array_values and
    // code_values. The valid keys are defined in the trait methods
    fn parse_until_next_section<'a>(
        &mut self,
        lines: &'a [&'a str],
    ) -> Result<&'a [&'a str]> {
        let mut array_parser = ArrayParser::new();
        let mut code_parser = CodeParser::new();
        let mut values: HashMap<&'static str, String> = HashMap::new();
        let mut array_values: HashMap<&'static str, Vec<String>> = HashMap::new();
        let mut code_values: HashMap<&'static str, String> = HashMap::new();
        let mut next_section: &'a [&str] = &[];
        for (index, line) in lines.iter().enumerate() {
            // If we are currently parsing a code value (`=(`), then take the line as it is
            // and pass it to the code_parser
            if code_parser.is_parsing {
                code_parser.parse_line(line);
                // Did we find a closing token?
                if code_parser.is_parsing {
                    // If not, go to the next line
                    continue;
                }
                // Otherwise add this value to the code_values
                add_field_value(
                    &code_parser.key,
                    code_parser.code,
                    &mut code_values,
                    self.get_code_fields(),
                )?;

                // Reset
                code_parser = CodeParser::new();
            }

            let line = line.trim();
            // Start testing for empty lines (we skip them) or for code values
            if is_empty_line(line)
                || code_parser
                    .start_parsing(line)
                    .with_context(|_| CodeParserSnafu)?
            {
                // The first line of the code values (`key =(`) do not contain any data
                // go to the next
                continue;
                // Checking for array parser here after
                // code_parser.start_parsing is fine because we
                // assume that no section can have arrays and code
                // change accordingly if this assumption change
            } else if (array_parser.is_parsing && {
                array_parser.parse_line(line).context(ArrayParserSnafu {
                    field: array_parser.key.to_owned(),
                })?;
                true
            }) || array_parser.start_parsing(line).context(ArrayParserSnafu {
                field: array_parser.key.to_owned(),
            })? {
                if array_parser.is_parsing {
                    continue;
                }

                let key = array_parser.key.to_owned();
                add_field_value(
                    &key,
                    array_parser.get_values().context(ArrayParserSnafu {
                        field: key.to_owned(),
                    })?,
                    &mut array_values,
                    self.get_array_fields(),
                )?;
                array_parser = ArrayParser::new();
                // Is this line a new section? Then break the loop
            } else if parse_section(line).is_some() {
                next_section = &lines[index..];
                break;
            } else if let Some((key, value)) = line.split_once('=') {
                add_field_value(
                    key.trim(),
                    value.trim().to_string(),
                    &mut values,
                    self.get_fields(),
                )?;
            }
        }

        // Check that all parsers state
        ensure!(
            !array_parser.is_parsing,
            ArrayNotClosedSnafu {
                field: array_parser.key
            }
        );

        self.build(&mut values, &mut array_values, &mut code_values);

        Ok(next_section)
    }
}
