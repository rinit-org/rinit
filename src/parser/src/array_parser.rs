pub struct ArrayParser {
    pub key: String,
    pub values: Vec<String>,
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
    ) -> Result<bool, String> {
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
            if !rest_of_line.starts_with(' ') {
                return Err("No space found after the token '['".to_string());
            }
            self.parse_line(rest_of_line.trim())?;
        }
        Ok(true)
    }

    pub fn parse_line(
        &mut self,
        line: &str,
    ) -> Result<(), String> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }

        let mut items: Vec<&str> = line.split(' ').collect();
        let count = items.iter().filter(|&n| *n == "]").count();
        if count > 1 {
            return Err(String::from("Multiple closing delimeters ']' found"));
        }

        let last = items.last().unwrap_or(&"");
        if count == 1 && last != &"]" {
            return Err(String::from("Values found after closing delimeter ']'"));
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array() {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [ ]").unwrap();
        assert_eq!(res, true);
        assert_eq!(parser.values, Vec::<String>::new());
        assert!(!parser.is_parsing);
    }

    #[test]
    fn parse_one_line_array() {
        let mut parser = ArrayParser::new();
        let res = parser.start_parsing("key = [ value1 value2 ]").unwrap();
        assert_eq!(res, true);

        assert_eq!(parser.key, "key");
        assert!(!parser.is_parsing);
        assert_eq!(
            parser.values,
            vec![String::from("value1"), String::from("value2")]
        );
    }

    #[test]
    fn parse_multiline_value() -> Result<(), String> {
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
    fn parse_value_and_ending_token_on_the_same_line() -> Result<(), String> {
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
        let err = parser.start_parsing("key = [value1 value2 ]").unwrap_err();
        assert_eq!(err, "No space found after the token '['");
    }
}
