use std::collections::BTreeSet;

const CATEGORY_KEYS: [&str; 12] = [
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_MESSAGES",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleEnvironment {
    pub lang: String,
    pub categories: Vec<(String, String)>,
    pub lc_all: String,
}

pub fn locale_environment_from_pairs<I, K, V>(pairs: I) -> LocaleEnvironment
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let map = pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let lang = map
        .get("LANG")
        .cloned()
        .unwrap_or_else(|| "C.UTF-8".to_string());
    let lc_all = map.get("LC_ALL").cloned().unwrap_or_default();
    let categories = CATEGORY_KEYS
        .iter()
        .map(|key| {
            (
                (*key).to_string(),
                map.get(*key).cloned().unwrap_or_default(),
            )
        })
        .collect();

    LocaleEnvironment {
        lang,
        categories,
        lc_all,
    }
}

pub fn current_locale_from_pairs<I, K, V>(pairs: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let map = pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(value) = map.get(key) {
            if !value.trim().is_empty() {
                return value.clone();
            }
        }
    }
    "C.UTF-8".to_string()
}

pub fn charmap_for_locale(locale: &str) -> &'static str {
    let locale = locale.to_ascii_uppercase();
    if locale == "C" || locale == "POSIX" {
        "ANSI_X3.4-1968"
    } else if locale.contains("UTF-8") || locale.contains("UTF8") {
        "UTF-8"
    } else {
        "ISO-8859-1"
    }
}

pub fn normalize_locale_name(value: &str) -> String {
    let trimmed = value.trim();
    let trimmed = trimmed.split_whitespace().next().unwrap_or(trimmed);
    if trimmed == "C" || trimmed == "POSIX" {
        return trimmed.to_string();
    }
    trimmed
        .replace(".UTF-8", ".utf8")
        .replace(".utf-8", ".utf8")
        .replace(".UTF8", ".utf8")
}

pub fn parse_supported_locale_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let mut fields = trimmed.split_whitespace();
    let locale = fields.next()?;
    let charset = fields.next()?;
    Some((normalize_locale_name(locale), charset.to_string()))
}

pub fn registry_locale_names(registry: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    for line in registry.lines() {
        let normalized = normalize_locale_name(line);
        if !normalized.is_empty() {
            names.insert(normalized);
        }
    }
    names.into_iter().collect()
}

pub fn category_keys() -> &'static [&'static str] {
    &CATEGORY_KEYS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_current_locale_order() {
        let locale =
            current_locale_from_pairs([("LANG", "en_US.UTF-8"), ("LC_CTYPE", "C"), ("LC_ALL", "")]);
        assert_eq!(locale, "C");
    }

    #[test]
    fn normalizes_utf8_spellings() {
        assert_eq!(normalize_locale_name("en_US.UTF-8 UTF-8"), "en_US.utf8");
        assert_eq!(normalize_locale_name("POSIX"), "POSIX");
    }

    #[test]
    fn parses_supported_rows() {
        assert_eq!(
            parse_supported_locale_line("en_US.UTF-8 UTF-8"),
            Some(("en_US.utf8".to_string(), "UTF-8".to_string()))
        );
        assert_eq!(parse_supported_locale_line("# comment"), None);
    }
}
