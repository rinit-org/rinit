use snafu::Snafu;

#[derive(Snafu, Debug, PartialEq, Eq)]
pub enum CodeParserError {
    #[snafu(display("the key must be non-empty"))]
    EmptyKey,
}

pub struct CodeParser {
    pub key: String,
    pub code: String,
    pub is_parsing: bool,
}

impl CodeParser {
    pub fn new() -> Self {
        CodeParser {
            key: String::new(),
            code: String::new(),
            is_parsing: false,
        }
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeParser {
    pub fn start_parsing(
        &mut self,
        line: &str,
    ) -> Result<bool, CodeParserError> {
        debug_assert!(!self.is_parsing);

        let line = line.trim();
        if let Some((key, token)) = line.split_once('=') {
            if token.is_empty() || token.trim_start() != "(" {
                return Ok(false);
            }

            self.key = key.trim().to_owned();
            self.is_parsing = true;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn parse_line(
        &mut self,
        line: &str,
    ) {
        if line.trim_end() == ")" {
            self.is_parsing = false;
        } else {
            self.code.push_str(line);
        }
    }
}
