pub fn parse_section(line: &str) -> Option<&str> {
    if line.chars().next().unwrap_or(' ') == '[' && line.chars().next_back().unwrap_or(' ') == ']' {
        Some(&line[1..line.len() - 1])
    } else {
        None
    }
}
