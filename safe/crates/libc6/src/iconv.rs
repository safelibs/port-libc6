use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConversionOptions {
    pub omit_invalid: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconvError {
    pub kind: IconvErrorKind,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IconvErrorKind {
    UnknownEncoding,
    InvalidSequence,
    IncompleteSequence,
    Unrepresentable,
}

impl IconvError {
    fn new(kind: IconvErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for IconvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl Error for IconvError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Encoding {
    Utf8,
    Ascii,
    Latin1,
    Utf16Le,
    Utf16Be,
}

pub fn convert_bytes(
    input: &[u8],
    from: &str,
    to: &str,
    options: ConversionOptions,
) -> Result<Vec<u8>, IconvError> {
    let from = parse_encoding(from)?;
    let to = parse_encoding(to)?;
    if from == to {
        return Ok(input.to_vec());
    }

    let text = decode_to_string(input, from, options)?;
    encode_from_string(&text, to, options)
}

pub fn normalized_encoding_name(value: &str) -> String {
    value
        .trim()
        .split("//")
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

fn parse_encoding(value: &str) -> Result<Encoding, IconvError> {
    match normalized_encoding_name(value).as_str() {
        "UTF8" => Ok(Encoding::Utf8),
        "ASCII" | "ANSIX341968" | "USASCII" => Ok(Encoding::Ascii),
        "ISO88591" | "LATIN1" => Ok(Encoding::Latin1),
        "UTF16LE" => Ok(Encoding::Utf16Le),
        "UTF16BE" => Ok(Encoding::Utf16Be),
        _ => Err(IconvError::new(
            IconvErrorKind::UnknownEncoding,
            format!("unsupported character encoding {value}"),
        )),
    }
}

fn decode_to_string(
    input: &[u8],
    encoding: Encoding,
    options: ConversionOptions,
) -> Result<String, IconvError> {
    match encoding {
        Encoding::Utf8 => decode_utf8(input, options.omit_invalid),
        Encoding::Ascii => decode_ascii(input, options.omit_invalid),
        Encoding::Latin1 => Ok(input.iter().map(|byte| *byte as char).collect()),
        Encoding::Utf16Le => decode_utf16(input, true, options.omit_invalid),
        Encoding::Utf16Be => decode_utf16(input, false, options.omit_invalid),
    }
}

fn decode_utf8(input: &[u8], omit_invalid: bool) -> Result<String, IconvError> {
    if !omit_invalid {
        return String::from_utf8(input.to_vec()).map_err(|error| {
            IconvError::new(
                IconvErrorKind::InvalidSequence,
                format!(
                    "invalid UTF-8 input at byte {}",
                    error.utf8_error().valid_up_to()
                ),
            )
        });
    }

    let mut output = String::new();
    let mut remaining = input;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(valid) => {
                output.push_str(valid);
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    output.push_str(std::str::from_utf8(&remaining[..valid_up_to]).map_err(
                        |_| {
                            IconvError::new(
                                IconvErrorKind::InvalidSequence,
                                "internal UTF-8 validation inconsistency",
                            )
                        },
                    )?);
                }
                let advance = error.error_len().unwrap_or(remaining.len() - valid_up_to);
                let next = valid_up_to.saturating_add(advance).min(remaining.len());
                if next == 0 {
                    break;
                }
                remaining = &remaining[next..];
            }
        }
    }
    Ok(output)
}

fn decode_ascii(input: &[u8], omit_invalid: bool) -> Result<String, IconvError> {
    let mut out = String::with_capacity(input.len());
    for byte in input {
        if byte.is_ascii() {
            out.push(*byte as char);
        } else if !omit_invalid {
            return Err(IconvError::new(
                IconvErrorKind::InvalidSequence,
                format!("invalid ASCII input byte 0x{byte:02x}"),
            ));
        }
    }
    Ok(out)
}

fn decode_utf16(
    input: &[u8],
    little_endian: bool,
    omit_invalid: bool,
) -> Result<String, IconvError> {
    let mut words = Vec::with_capacity(input.len() / 2);
    let mut chunks = input.chunks_exact(2);
    for chunk in &mut chunks {
        let word = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        words.push(word);
    }
    if !chunks.remainder().is_empty() && !omit_invalid {
        return Err(IconvError::new(
            IconvErrorKind::IncompleteSequence,
            "incomplete UTF-16 input unit",
        ));
    }

    let mut out = String::new();
    for item in char::decode_utf16(words) {
        match item {
            Ok(ch) => out.push(ch),
            Err(_) if omit_invalid => {}
            Err(_) => {
                return Err(IconvError::new(
                    IconvErrorKind::InvalidSequence,
                    "invalid UTF-16 input sequence",
                ))
            }
        }
    }
    Ok(out)
}

fn encode_from_string(
    text: &str,
    encoding: Encoding,
    options: ConversionOptions,
) -> Result<Vec<u8>, IconvError> {
    match encoding {
        Encoding::Utf8 => Ok(text.as_bytes().to_vec()),
        Encoding::Ascii => encode_ascii(text, options.omit_invalid),
        Encoding::Latin1 => encode_latin1(text, options.omit_invalid),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for word in text.encode_utf16() {
                let bytes = if encoding == Encoding::Utf16Le {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                };
                out.extend(bytes);
            }
            Ok(out)
        }
    }
}

fn encode_ascii(text: &str, omit_invalid: bool) -> Result<Vec<u8>, IconvError> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch as u8);
        } else if !omit_invalid {
            return Err(IconvError::new(
                IconvErrorKind::Unrepresentable,
                format!(
                    "character U+{:04X} cannot be represented as ASCII",
                    ch as u32
                ),
            ));
        }
    }
    Ok(out)
}

fn encode_latin1(text: &str, omit_invalid: bool) -> Result<Vec<u8>, IconvError> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code <= 0xff {
            out.push(code as u8);
        } else if !omit_invalid {
            return Err(IconvError::new(
                IconvErrorKind::Unrepresentable,
                format!(
                    "character U+{:04X} cannot be represented as ISO-8859-1",
                    code
                ),
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_latin1_to_utf8() {
        let converted = convert_bytes(
            b"caf\xe9",
            "ISO-8859-1",
            "UTF-8",
            ConversionOptions::default(),
        )
        .unwrap();
        assert_eq!(converted, "cafe".replace('e', "\u{00e9}").as_bytes());
    }

    #[test]
    fn rejects_unknown_encodings() {
        let error =
            convert_bytes(b"x", "UNKNOWN", "UTF-8", ConversionOptions::default()).unwrap_err();
        assert_eq!(error.kind, IconvErrorKind::UnknownEncoding);
    }

    #[test]
    fn omits_invalid_utf8_with_forward_progress() {
        let converted = convert_bytes(
            b"a\xf0\x28\x8c\x28b",
            "UTF-8",
            "ASCII",
            ConversionOptions { omit_invalid: true },
        )
        .unwrap();
        assert_eq!(converted, b"a((b");
    }
}
