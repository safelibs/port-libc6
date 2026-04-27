use std::io::{self, Read};

pub fn read_to_end<R: Read>(reader: &mut R, limit: usize) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    while buffer.len() < limit {
        let read = reader.read(&mut chunk[..chunk.len().min(limit - buffer.len())])?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_to_end_respects_limit() {
        let mut input = &b"abcdef"[..];
        assert_eq!(read_to_end(&mut input, 3).unwrap(), b"abc");
    }
}
