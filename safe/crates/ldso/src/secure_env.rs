use std::path::Path;

const LOADER_SENSITIVE_PREFIXES: &[&str] = &["LD_", "GLIBC_TUNABLES"];
const ALWAYS_STRIP_IN_SECURE_EXEC: &[&str] = &[
    "GCONV_PATH",
    "GETCONF_DIR",
    "HOSTALIASES",
    "LD_AUDIT",
    "LD_ASSUME_KERNEL",
    "LD_BIND_NOT",
    "LD_BIND_NOW",
    "LD_DEBUG",
    "LD_DEBUG_OUTPUT",
    "LD_DYNAMIC_WEAK",
    "LD_HWCAP_MASK",
    "LD_LIBRARY_PATH",
    "LD_ORIGIN_PATH",
    "LD_POINTER_GUARD",
    "LD_PRELOAD",
    "LD_PROFILE",
    "LD_SHOW_AUXV",
    "LD_TRACE_LOADED_OBJECTS",
    "LD_USE_LOAD_BIAS",
    "LD_VERBOSE",
    "LD_WARN",
    "LD_PREFER_MAP_32BIT_EXEC",
    "LOCALDOMAIN",
    "LOCPATH",
    "MALLOC_TRACE",
    "NIS_PATH",
    "NLSPATH",
    "RESOLV_HOST_CONF",
    "RES_OPTIONS",
    "TMPDIR",
    "TZDIR",
];

pub fn is_loader_sensitive_env(name: &str) -> bool {
    LOADER_SENSITIVE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || ALWAYS_STRIP_IN_SECURE_EXEC.contains(&name)
}

pub fn secure_exec_env() -> Vec<(String, String)> {
    secure_exec_env_from_pairs(std::env::vars())
}

pub fn secure_exec_env_from_pairs<I, K, V>(pairs: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .filter(|(key, _)| !is_loader_sensitive_env(key))
        .collect()
}

pub fn expand_dynamic_tokens(value: &str, origin: &Path, platform: Option<&str>) -> String {
    let mut output = value.replace("$ORIGIN", &origin.display().to_string());
    output = output.replace("${ORIGIN}", &origin.display().to_string());
    output = output.replace("$LIB", "lib64");
    output = output.replace("${LIB}", "lib64");
    if let Some(platform) = platform {
        output = output.replace("$PLATFORM", platform);
        output = output.replace("${PLATFORM}", platform);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_loader_sensitive_variables_in_secure_exec() {
        let filtered = secure_exec_env_from_pairs([
            ("PATH", "/usr/bin"),
            ("LD_LIBRARY_PATH", "/tmp/lib"),
            ("GLIBC_TUNABLES", "glibc.cpu.hwcaps=-AVX2"),
            ("LANG", "C"),
        ]);
        assert_eq!(
            filtered,
            vec![
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("LANG".to_string(), "C".to_string()),
            ]
        );
    }

    #[test]
    fn expands_origin_lib_and_platform_tokens() {
        let origin = PathBuf::from("/srv/app/lib");
        let expanded = expand_dynamic_tokens("$ORIGIN:$LIB:${PLATFORM}", &origin, Some("x86_64"));
        assert_eq!(expanded, "/srv/app/lib:lib64:x86_64");
    }
}
