use ldso::{parse_auxv, AuxEntry, AuxValues};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialStack {
    pub argc: usize,
    pub argv: Vec<String>,
    pub envp: Vec<(String, String)>,
    pub auxv: AuxValues,
}

pub fn flatten_initial_stack(
    argc: usize,
    argv: &[String],
    envp: &[(String, String)],
    auxv_entries: &[AuxEntry],
) -> InitialStack {
    InitialStack {
        argc,
        argv: argv.to_vec(),
        envp: envp.to_vec(),
        auxv: parse_auxv(auxv_entries),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ldso::{AT_NULL, AT_SECURE};

    #[test]
    fn builds_startup_stack_snapshot() {
        let stack = flatten_initial_stack(
            2,
            &["prog".to_string(), "--help".to_string()],
            &[("LANG".to_string(), "C".to_string())],
            &[
                AuxEntry {
                    key: AT_SECURE,
                    value: 1,
                },
                AuxEntry {
                    key: AT_NULL,
                    value: 0,
                },
            ],
        );
        assert_eq!(stack.argc, 2);
        assert_eq!(stack.argv[0], "prog");
        assert_eq!(stack.envp[0].0, "LANG");
        assert!(stack.auxv.secure());
    }
}
