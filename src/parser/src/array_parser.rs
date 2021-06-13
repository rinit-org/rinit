use snafu::{
    ensure,
    Snafu,
};

#[derive(Debug, Snafu, PartialEq)]
pub enum ArrayParserError {
    #[snafu(display("values {:?} are duplicated", values))]
    DuplicatedValuesFound { values: Vec<String> },
    #[snafu(display("the array is empty"))]
    EmptyArray,
    #[snafu(display("No space found after the token '['"))]
    NoSpaceAfterStartTokenError,
    #[snafu(display("Multiple closing delimeters ']' found"))]
    MultipleClosingDelimiterFoundError,
    #[snafu(display("Values found after closing delimeter ']'"))]
    ValuesFoundAfterEndingDelimeterError,
}

pub struct ArrayParser {
    pub key: String,
    values: Vec<String>,
    pub is_parsing: bool,
}

impl ArrayParser {
    pub fn new() -> ArrayParser {
        ArrayParser {
            key: String::new(),
            values: Vec::new(),
            is_parsing: false,
        }
    }
}

impl Default for ArrayParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrayParser {
    pub fn start_parsing(
        &mut self,
        line: &str,
    ) -> Result<bool, ArrayParserError> {
        let eq_token_index = line.find('=');
        if eq_token_index == None {
            // this is an error, but let the key_value parser throws
            return Ok(false);
        }
        let eq_token_index = eq_token_index.unwrap();

        let bracket_token_index = line.find('[');
        if bracket_token_index.is_none() {
            // this line does not contain an array
            return Ok(false);
        }
        let bracket_token_index = bracket_token_index.unwrap();

        if line[eq_token_index..bracket_token_index].trim() == "" {
            // there are characters between '=' and '[', so this is not an array
            return Ok(false);
        }
        self.key = String::from(line[..eq_token_index].trim());
        self.is_parsing = true;
        let rest_of_line = &line[bracket_token_index + 1..];
        if !rest_of_line.is_empty() {
            ensure!(rest_of_line.starts_with(' '), NoSpaceAfterStartTokenError);
            self.parse_line(rest_of_line.trim())?;
        }
        Ok(true)
    }

    pub fn parse_line(
        &mut self,
        line: &str,
    ) -> Result<(), ArrayParserError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }

        let mut items: Vec<&str> = line.split(' ').collect();
        let count = items.iter().filter(|&n| *n == "]").count();
        ensure!(count < 2, MultipleClosingDelimiterFoundError);

        ensure!(
            count != 1 || items.last().unwrap_or(&"") == &"]",
            ValuesFoundAfterEndingDelimeterError
        );

        if count == 1 {
            items.pop();
            self.is_parsing = false;
        }

        if !items.is_empty() {
            let new_values = &mut items.iter().map(|&item| String::from(item)).collect();
            self.values.append(new_values);
        }

        Ok(())
    }

    pub fn get_values(mut self) -> Result<Vec<String>, ArrayParserError> {
        self.values.sort_unstable();
        let (_, dups) = self.values.partition_dedup();
        ensure!(
            dups.is_empty(),
            DuplicatedValuesFound {
                values: dups.to_vec()
            }
        );
        ensure!(self.values.len() != 0, EmptyArray {});
        Ok(self.values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array() -> Result<(), ArrayParserError> {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [ ]")?;
        assert_eq!(res, true);
        assert_eq!(parser.values, Vec::<String>::new());
        assert!(!parser.is_parsing);
        Ok(())
    }

    #[test]
    fn parse_one_line_array() -> Result<(), ArrayParserError> {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [ value1 value2 ]")?;
        assert_eq!(res, true);

        assert_eq!(parser.key, "key");
        assert!(!parser.is_parsing);
        assert_eq!(
            parser.values,
            vec![String::from("value1"), String::from("value2")]
        );
        Ok(())
    }

    #[test]
    fn parse_multiline_value() -> Result<(), ArrayParserError> {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [")?;
        assert!(res);
        parser.parse_line("value1")?;
        assert!(parser.is_parsing);
        parser.parse_line("]")?;

        assert_eq!(parser.key, "key");
        assert!(!parser.is_parsing);
        assert_eq!(parser.values, vec!["value1".to_string()]);

        Ok(())
    }

    #[test]
    fn parse_value_and_ending_token_on_the_same_line() -> Result<(), ArrayParserError> {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [")?;
        assert!(res);
        parser.parse_line("value1")?;
        assert!(parser.is_parsing);
        parser.parse_line("value2 ]")?;

        assert_eq!(parser.key, "key");
        assert!(!parser.is_parsing);
        assert_eq!(
            parser.values,
            vec!["value1".to_string(), "value2".to_string()]
        );

        Ok(())
    }

    #[test]
    fn error_no_space_after() {
        let mut parser = ArrayParser::new();
        let err = parser.start_parsing("key = [value1 value2 ]");
        assert_eq!(err, Err(ArrayParserError::NoSpaceAfterStartTokenError));
    }
}
