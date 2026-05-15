use glob::glob;
use regex::Regex;
use std::env;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FnmatchFlags {
    pub pathname: bool,
    pub no_escape: bool,
    pub period: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParserErrorKind {
    Regex,
    CommandSubstitution,
    UnterminatedQuote,
    Glob,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParserError {
    pub kind: ParserErrorKind,
    pub detail: String,
}

impl ParserError {
    fn new(kind: ParserErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl Error for ParserError {}

pub fn fnmatch(pattern: &str, candidate: &str, flags: FnmatchFlags) -> Result<bool, ParserError> {
    if flags.period && candidate.starts_with('.') && !pattern.starts_with('.') {
        return Ok(false);
    }
    let regex = Regex::new(&fnmatch_regex(pattern, flags)?).map_err(|error| {
        ParserError::new(
            ParserErrorKind::Regex,
            format!("failed to compile fnmatch pattern: {error}"),
        )
    })?;
    Ok(regex.is_match(candidate))
}

pub fn compile_posix_regex(pattern: &str, extended: bool) -> Result<Regex, ParserError> {
    let translated = if extended {
        pattern.to_string()
    } else {
        translate_basic_regex(pattern)
    };
    Regex::new(&translated).map_err(|error| {
        ParserError::new(
            ParserErrorKind::Regex,
            format!("failed to compile POSIX regex: {error}"),
        )
    })
}

pub fn glob_paths(pattern: &str) -> Result<Vec<PathBuf>, ParserError> {
    let entries = glob(pattern).map_err(|error| {
        ParserError::new(
            ParserErrorKind::Glob,
            format!("failed to parse glob pattern: {error}"),
        )
    })?;
    let mut paths = entries.filter_map(Result::ok).collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub fn wordexp_no_cmd(input: &str) -> Result<Vec<String>, ParserError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, '`') => {
                return Err(ParserError::new(
                    ParserErrorKind::CommandSubstitution,
                    "command substitution is disabled",
                ));
            }
            (None, '$') if chars.peek() == Some(&'(') => {
                return Err(ParserError::new(
                    ParserErrorKind::CommandSubstitution,
                    "command substitution is disabled",
                ));
            }
            (None, '\'') | (None, '"') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (_, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (_, '$') => {
                current.push_str(&read_env_expansion(&mut chars));
            }
            (_, c) => current.push(c),
        }
    }

    if quote.is_some() {
        return Err(ParserError::new(
            ParserErrorKind::UnterminatedQuote,
            "unterminated quote in word expansion input",
        ));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

fn read_env_expansion<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let mut name = String::new();
    while let Some(ch) = chars.peek().copied() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            name.push(ch);
            let _ = chars.next();
        } else {
            break;
        }
    }
    if name.is_empty() {
        "$".to_string()
    } else {
        env::var(name).unwrap_or_default()
    }
}

fn fnmatch_regex(pattern: &str, flags: FnmatchFlags) -> Result<String, ParserError> {
    let mut out = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => {
                if flags.pathname {
                    out.push_str("[^/]*");
                } else {
                    out.push_str(".*");
                }
            }
            '?' => {
                if flags.pathname {
                    out.push_str("[^/]");
                } else {
                    out.push('.');
                }
            }
            '[' => {
                out.push_str(&translate_bracket(&mut chars, flags.pathname));
            }
            '\\' if !flags.no_escape => {
                if let Some(next) = chars.next() {
                    out.push_str(&regex::escape(&next.to_string()));
                } else {
                    out.push_str("\\\\");
                }
            }
            c => out.push_str(&regex::escape(&c.to_string())),
        }
    }
    out.push('$');
    Ok(out)
}

fn translate_bracket<I>(chars: &mut std::iter::Peekable<I>, pathname: bool) -> String
where
    I: Iterator<Item = char>,
{
    let mut raw = String::new();
    let mut closed = false;
    if let Some(ch @ ('!' | '^')) = chars.peek().copied() {
        raw.push(ch);
        let _ = chars.next();
    }
    while let Some(ch) = chars.next() {
        if ch == ']' && !raw.is_empty() {
            closed = true;
            break;
        }
        raw.push(ch);
    }
    if !closed {
        return format!("\\[{}", regex::escape(&raw));
    }

    let negated = raw.starts_with('!') || raw.starts_with('^');
    if negated {
        raw.remove(0);
    }

    let mut out = String::from("[");
    if negated {
        out.push('^');
        if pathname {
            out.push('/');
        }
    }
    out.push_str(&raw);
    out.push(']');
    out
}

fn translate_basic_regex(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('(' | ')' | '{' | '}' | '+' | '?' | '|') => {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                    let _ = chars.next();
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_handles_globs_without_recursion() {
        assert!(fnmatch("link_*", "link_compat", FnmatchFlags::default()).unwrap());
        assert!(!fnmatch(
            "link_*",
            "other",
            FnmatchFlags {
                pathname: false,
                no_escape: false,
                period: false
            }
        )
        .unwrap());
    }

    #[test]
    fn wordexp_rejects_command_substitution() {
        let error = wordexp_no_cmd("safe $(rm -rf /)").unwrap_err();
        assert_eq!(error.kind, ParserErrorKind::CommandSubstitution);
    }

    #[test]
    fn wordexp_reports_unterminated_quotes() {
        let error = wordexp_no_cmd("'unterminated").unwrap_err();
        assert_eq!(error.kind, ParserErrorKind::UnterminatedQuote);
    }
}
