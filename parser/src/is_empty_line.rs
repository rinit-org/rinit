pub fn is_empty_line(line: &str) -> bool {
    if let Some(char) = line.chars().next() {
        char == '#'
    } else {
        true
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn line_empty() {
        assert!(is_empty_line(""));
        assert!(is_empty_line("# comment"));
    }

    #[test]
    fn non_empty_line() {
        assert!(!is_empty_line("contents = [ foo ]"));
    }
}
