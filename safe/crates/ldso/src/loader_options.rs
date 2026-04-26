#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoaderMode {
    Execute {
        program: String,
        program_args: Vec<String>,
    },
    Help,
    ListTunables,
    Verify {
        object: String,
    },
    Version,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LoaderInvocation {
    pub argv0: Option<String>,
    pub audit: Option<String>,
    pub glibc_hwcaps_mask: Option<String>,
    pub glibc_hwcaps_prepend: Option<String>,
    pub inhibit_cache: bool,
    pub inhibit_rpath: Option<String>,
    pub library_path: Option<String>,
    pub preload: Option<String>,
    pub mode: Option<LoaderMode>,
    pub passthrough: Vec<String>,
}

impl LoaderInvocation {
    pub fn parse(args: &[String]) -> Self {
        let mut invocation = LoaderInvocation::default();
        let mut iter = args.iter().peekable();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--argv0" => {
                    invocation.argv0 = iter.next().cloned();
                }
                "--audit" => {
                    invocation.audit = iter.next().cloned();
                }
                "--glibc-hwcaps-mask" => {
                    invocation.glibc_hwcaps_mask = iter.next().cloned();
                }
                "--glibc-hwcaps-prepend" => {
                    invocation.glibc_hwcaps_prepend = iter.next().cloned();
                }
                "--inhibit-cache" => {
                    invocation.inhibit_cache = true;
                }
                "--inhibit-rpath" => {
                    invocation.inhibit_rpath = iter.next().cloned();
                }
                "--library-path" => {
                    invocation.library_path = iter.next().cloned();
                }
                "--list-tunables" => {
                    invocation.mode = Some(LoaderMode::ListTunables);
                    invocation.passthrough.extend(iter.cloned());
                    break;
                }
                "--preload" => {
                    invocation.preload = iter.next().cloned();
                }
                "--verify" => {
                    let object = iter.next().cloned().unwrap_or_default();
                    invocation.mode = Some(LoaderMode::Verify { object });
                    invocation.passthrough.extend(iter.cloned());
                    break;
                }
                "--help" => {
                    invocation.mode = Some(LoaderMode::Help);
                    invocation.passthrough.extend(iter.cloned());
                    break;
                }
                "--version" => {
                    invocation.mode = Some(LoaderMode::Version);
                    invocation.passthrough.extend(iter.cloned());
                    break;
                }
                other if other.starts_with('-') => {
                    invocation.passthrough.push(other.to_string());
                }
                program => {
                    let mut program_args = iter.cloned().collect::<Vec<_>>();
                    invocation.mode = Some(LoaderMode::Execute {
                        program: program.to_string(),
                        program_args: std::mem::take(&mut program_args),
                    });
                    break;
                }
            }
        }

        invocation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loader_execute_mode_and_options() {
        let args = vec![
            "--library-path".to_string(),
            "/tmp/lib".to_string(),
            "--glibc-hwcaps-prepend".to_string(),
            "x86-64-v3".to_string(),
            "/bin/echo".to_string(),
            "hello".to_string(),
        ];
        let parsed = LoaderInvocation::parse(&args);
        assert_eq!(parsed.library_path.as_deref(), Some("/tmp/lib"));
        assert_eq!(parsed.glibc_hwcaps_prepend.as_deref(), Some("x86-64-v3"));
        assert_eq!(
            parsed.mode,
            Some(LoaderMode::Execute {
                program: "/bin/echo".to_string(),
                program_args: vec!["hello".to_string()],
            })
        );
    }

    #[test]
    fn parses_list_tunables_mode() {
        let parsed = LoaderInvocation::parse(&["--list-tunables".to_string()]);
        assert_eq!(parsed.mode, Some(LoaderMode::ListTunables));
    }
}
