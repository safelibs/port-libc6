pub fn bounded_strlen(bytes: &[u8], max_len: usize) -> usize {
    bytes
        .iter()
        .take(max_len)
        .position(|byte| *byte == 0)
        .unwrap_or(max_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_at_nul() {
        assert_eq!(bounded_strlen(b"abc\0rest", 16), 3);
    }
}
