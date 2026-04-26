use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunableKind {
    String,
    Int32 { min: i64, max: i64 },
    Uint64 { min: u64, max: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunableValue {
    String(String),
    Integer(i64),
    Unsigned(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TunableDefinition {
    pub name: &'static str,
    pub env_alias: Option<&'static str>,
    pub kind: TunableKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TunablesState {
    values: BTreeMap<String, TunableValue>,
}

impl TunablesState {
    pub fn get(&self, name: &str) -> Option<&TunableValue> {
        self.values.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: TunableValue) {
        self.values.insert(name.into(), value);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &TunableValue)> {
        self.values.iter()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TunableRegistry {
    defs: Vec<TunableDefinition>,
}

impl TunableRegistry {
    pub fn new(defs: Vec<TunableDefinition>) -> Self {
        Self { defs }
    }

    pub fn definitions(&self) -> &[TunableDefinition] {
        &self.defs
    }

    pub fn find(&self, name: &str) -> Option<&TunableDefinition> {
        self.defs.iter().find(|item| item.name == name)
    }

    pub fn parse_env<I>(&self, secure: bool, env: I) -> TunablesState
    where
        I: IntoIterator<Item = (String, String)>,
    {
        if secure {
            return TunablesState::default();
        }

        let mut state = TunablesState::default();
        for (name, value) in env {
            if name == "GLIBC_TUNABLES" {
                for (tunable_name, raw_value) in parse_tunables_assignments(&value) {
                    if let Some(def) = self.find(&tunable_name) {
                        if let Some(parsed) = parse_value(def, &raw_value) {
                            state.insert(def.name, parsed);
                        }
                    }
                }
                continue;
            }

            for def in &self.defs {
                if def.env_alias == Some(name.as_str()) {
                    if let Some(parsed) = parse_value(def, &value) {
                        state.insert(def.name, parsed);
                    }
                }
            }
        }
        state
    }
}

pub fn parse_tunables_assignments(value: &str) -> Vec<(String, String)> {
    if value.is_empty() {
        return Vec::new();
    }

    value
        .split(':')
        .filter_map(|entry| {
            let (name, raw) = entry.split_once('=')?;
            if name.is_empty() {
                return None;
            }
            Some((name.to_string(), raw.to_string()))
        })
        .collect()
}

pub fn default_tunable_registry() -> TunableRegistry {
    TunableRegistry::new(vec![
        TunableDefinition {
            name: "glibc.cpu.hwcaps",
            env_alias: None,
            kind: TunableKind::String,
        },
        TunableDefinition {
            name: "glibc.cpu.plt_rewrite",
            env_alias: None,
            kind: TunableKind::Int32 { min: 0, max: 2 },
        },
        TunableDefinition {
            name: "glibc.cpu.prefer_map_32bit_exec",
            env_alias: Some("LD_PREFER_MAP_32BIT_EXEC"),
            kind: TunableKind::Int32 { min: 0, max: 1 },
        },
        TunableDefinition {
            name: "glibc.cpu.x86_ibt",
            env_alias: None,
            kind: TunableKind::String,
        },
        TunableDefinition {
            name: "glibc.cpu.x86_shstk",
            env_alias: None,
            kind: TunableKind::String,
        },
    ])
}

fn parse_value(def: &TunableDefinition, raw: &str) -> Option<TunableValue> {
    match def.kind {
        TunableKind::String => Some(TunableValue::String(raw.to_string())),
        TunableKind::Int32 { min, max } => {
            let parsed = raw.parse::<i64>().ok()?;
            if parsed < min || parsed > max {
                return None;
            }
            Some(TunableValue::Integer(parsed))
        }
        TunableKind::Uint64 { min, max } => {
            let parsed = raw.parse::<u64>().ok()?;
            if parsed < min || parsed > max {
                return None;
            }
            Some(TunableValue::Unsigned(parsed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_glibc_tunables_assignments() {
        assert_eq!(
            parse_tunables_assignments("glibc.cpu.hwcaps=-AVX2:glibc.cpu.plt_rewrite=2"),
            vec![
                ("glibc.cpu.hwcaps".to_string(), "-AVX2".to_string()),
                ("glibc.cpu.plt_rewrite".to_string(), "2".to_string()),
            ]
        );
    }

    #[test]
    fn ignores_tunables_in_secure_mode() {
        let registry = default_tunable_registry();
        let state = registry.parse_env(
            true,
            vec![(
                "GLIBC_TUNABLES".to_string(),
                "glibc.cpu.plt_rewrite=2".to_string(),
            )],
        );
        assert!(state.iter().next().is_none());
    }

    #[test]
    fn parses_env_aliases_and_glibc_tunables() {
        let registry = default_tunable_registry();
        let state = registry.parse_env(
            false,
            vec![
                (
                    "GLIBC_TUNABLES".to_string(),
                    "glibc.cpu.hwcaps=-AVX2:glibc.cpu.plt_rewrite=2".to_string(),
                ),
                ("LD_PREFER_MAP_32BIT_EXEC".to_string(), "1".to_string()),
            ],
        );
        assert_eq!(
            state.get("glibc.cpu.hwcaps"),
            Some(&TunableValue::String("-AVX2".to_string()))
        );
        assert_eq!(
            state.get("glibc.cpu.plt_rewrite"),
            Some(&TunableValue::Integer(2))
        );
        assert_eq!(
            state.get("glibc.cpu.prefer_map_32bit_exec"),
            Some(&TunableValue::Integer(1))
        );
    }
}
