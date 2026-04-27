pub fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_windows_newlines() {
        assert_eq!(normalize_newlines("a\r\nb\r\n"), "a\nb\n");
    }
}
