use crate::startup::InitialStack;
use ldso::{
    current_process_auxv, default_tunable_registry, secure_exec_env, secure_exec_env_from_pairs,
    TunablesState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupContext {
    pub secure_exec: bool,
    pub tunables: TunablesState,
    pub envp: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupSnapshot {
    pub stack: InitialStack,
    pub context: StartupContext,
}

impl StartupContext {
    pub fn from_initial_stack(stack: &InitialStack) -> Self {
        let secure_exec = stack.auxv.secure();
        let envp = if secure_exec {
            secure_exec_env_from_pairs(stack.envp.clone())
        } else {
            stack.envp.clone()
        };
        let tunables = default_tunable_registry().parse_env(secure_exec, envp.clone());
        Self {
            secure_exec,
            tunables,
            envp,
        }
    }

    pub fn capture_current_process() -> anyhow::Result<Self> {
        let auxv = current_process_auxv()?;
        let secure_exec = auxv.secure();
        let envp = if secure_exec {
            secure_exec_env()
        } else {
            std::env::vars().collect()
        };
        let tunables = default_tunable_registry().parse_env(secure_exec, envp.clone());
        Ok(Self {
            secure_exec,
            tunables,
            envp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::flatten_initial_stack;
    use ldso::{AuxEntry, AT_NULL, AT_SECURE};

    #[test]
    fn filters_environment_for_secure_exec() {
        let stack = flatten_initial_stack(
            1,
            &["prog".to_string()],
            &[
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("LD_LIBRARY_PATH".to_string(), "/tmp/lib".to_string()),
                (
                    "GLIBC_TUNABLES".to_string(),
                    "glibc.cpu.plt_rewrite=2".to_string(),
                ),
            ],
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
        let context = StartupContext::from_initial_stack(&stack);
        assert!(context.secure_exec);
        assert_eq!(
            context.envp,
            vec![("PATH".to_string(), "/usr/bin".to_string())]
        );
        assert!(context.tunables.iter().next().is_none());
    }
}
