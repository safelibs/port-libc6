use crate::loader_tools::{
    LDCONFIG_BACKEND_INSTALL_PATH, LDSO_BACKEND_INSTALL_PATH, LOADER_TOOL_BINARY_NAME,
};
use crate::runtime_tools::{PLDD_BACKEND_INSTALL_PATH, RUNTIME_TOOL_BINARY_NAME};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendAsset {
    pub install_path: &'static str,
    pub source_path: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequiredToolKind {
    FallbackWrapper {
        fallback_source_path: &'static str,
    },
    RustEntrypoint {
        binary_name: &'static str,
        public_source_path: &'static str,
        backend_assets: &'static [BackendAsset],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequiredTool {
    pub package: &'static str,
    pub entrypoint: &'static str,
    pub owner_phase: &'static str,
    pub verification: &'static str,
    pub kind: RequiredToolKind,
}

const LDSO_BACKEND_ASSETS: &[BackendAsset] = &[BackendAsset {
    install_path: LDSO_BACKEND_INSTALL_PATH,
    source_path: "build/testroot.pristine/usr/lib64/ld-linux-x86-64.so.2",
}];

const LDCONFIG_BACKEND_ASSETS: &[BackendAsset] = &[BackendAsset {
    install_path: LDCONFIG_BACKEND_INSTALL_PATH,
    source_path: "build/testroot.pristine/usr/sbin/ldconfig",
}];

const PLDD_BACKEND_ASSETS: &[BackendAsset] = &[BackendAsset {
    install_path: PLDD_BACKEND_INSTALL_PATH,
    source_path: "build/testroot.pristine/usr/bin/pldd",
}];

const REQUIRED_TOOLS: &[RequiredTool] = &[
    RequiredTool {
        package: "libc-dev-bin",
        entrypoint: "/usr/bin/gencat",
        owner_phase: "impl_09_math_and_aux_dsos",
        verification: "dev-and-time-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/gencat",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/getconf",
        owner_phase: "impl_09_math_and_aux_dsos",
        verification: "dev-and-time-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/getconf",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/getent",
        owner_phase: "impl_07_nss_resolver_nscd",
        verification: "network-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/getent",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/iconv",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/iconv",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/ld.so",
        owner_phase: "impl_04_loader_startup_secure_exec",
        verification: "loader-tools",
        kind: RequiredToolKind::RustEntrypoint {
            binary_name: LOADER_TOOL_BINARY_NAME,
            public_source_path: "safe/crates/libc-support-tools/src/loader_tools.rs",
            backend_assets: LDSO_BACKEND_ASSETS,
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/ldd",
        owner_phase: "impl_04_loader_startup_secure_exec",
        verification: "loader-tools",
        kind: RequiredToolKind::RustEntrypoint {
            binary_name: LOADER_TOOL_BINARY_NAME,
            public_source_path: "safe/crates/libc-support-tools/src/loader_tools.rs",
            backend_assets: LDSO_BACKEND_ASSETS,
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/locale",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/locale",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/localedef",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/localedef",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/pldd",
        owner_phase: "impl_05_core_runtime_threads_entropy",
        verification: "runtime-tools",
        kind: RequiredToolKind::RustEntrypoint {
            binary_name: RUNTIME_TOOL_BINARY_NAME,
            public_source_path: "safe/crates/libc-support-tools/src/runtime_tools.rs",
            backend_assets: PLDD_BACKEND_ASSETS,
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/tzselect",
        owner_phase: "impl_09_math_and_aux_dsos",
        verification: "dev-and-time-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/tzselect",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/bin/zdump",
        owner_phase: "impl_09_math_and_aux_dsos",
        verification: "dev-and-time-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/bin/zdump",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/sbin/iconvconfig",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/sbin/iconvconfig",
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/sbin/ldconfig",
        owner_phase: "impl_04_loader_startup_secure_exec",
        verification: "loader-tools",
        kind: RequiredToolKind::RustEntrypoint {
            binary_name: LOADER_TOOL_BINARY_NAME,
            public_source_path: "safe/crates/libc-support-tools/src/loader_tools.rs",
            backend_assets: LDCONFIG_BACKEND_ASSETS,
        },
    },
    RequiredTool {
        package: "libc-bin",
        entrypoint: "/usr/sbin/zic",
        owner_phase: "impl_09_math_and_aux_dsos",
        verification: "dev-and-time-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/sbin/zic",
        },
    },
    RequiredTool {
        package: "locales",
        entrypoint: "/usr/sbin/locale-gen",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "safe/debian/local/usr_sbin/locale-gen",
        },
    },
    RequiredTool {
        package: "locales",
        entrypoint: "/usr/sbin/update-locale",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "safe/debian/local/usr_sbin/update-locale",
        },
    },
    RequiredTool {
        package: "locales",
        entrypoint: "/usr/sbin/validlocale",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "safe/debian/local/usr_sbin/validlocale",
        },
    },
    RequiredTool {
        package: "locales",
        entrypoint: "/usr/share/locales/install-language-pack",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "safe/debian/local/usr_share_locales/install-language-pack",
        },
    },
    RequiredTool {
        package: "locales",
        entrypoint: "/usr/share/locales/remove-language-pack",
        owner_phase: "impl_08_locale_iconv_posix_parsers",
        verification: "locale-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "safe/debian/local/usr_share_locales/remove-language-pack",
        },
    },
    RequiredTool {
        package: "nscd",
        entrypoint: "/usr/sbin/nscd",
        owner_phase: "impl_07_nss_resolver_nscd",
        verification: "network-tools",
        kind: RequiredToolKind::FallbackWrapper {
            fallback_source_path: "build/testroot.pristine/usr/sbin/nscd",
        },
    },
];

pub fn required_tools() -> &'static [RequiredTool] {
    REQUIRED_TOOLS
}

pub fn find_required_tool(entrypoint: &str) -> Option<&'static RequiredTool> {
    REQUIRED_TOOLS
        .iter()
        .find(|tool| tool.entrypoint == entrypoint)
}

pub fn logical_source_path(tool: &RequiredTool) -> &'static str {
    match tool.kind {
        RequiredToolKind::FallbackWrapper {
            fallback_source_path,
        } => fallback_source_path,
        RequiredToolKind::RustEntrypoint {
            public_source_path, ..
        } => public_source_path,
    }
}

pub fn tool_binary_name(tool: &RequiredTool) -> Option<&'static str> {
    match tool.kind {
        RequiredToolKind::RustEntrypoint { binary_name, .. } => Some(binary_name),
        RequiredToolKind::FallbackWrapper { .. } => None,
    }
}

pub fn backend_assets(tool: &RequiredTool) -> &'static [BackendAsset] {
    match tool.kind {
        RequiredToolKind::RustEntrypoint { backend_assets, .. } => backend_assets,
        RequiredToolKind::FallbackWrapper { .. } => &[],
    }
}

pub fn fallback_asset_path(tool: &RequiredTool) -> Option<String> {
    let RequiredToolKind::FallbackWrapper { .. } = tool.kind else {
        return None;
    };
    let leaf = tool
        .entrypoint
        .trim_start_matches('/')
        .rsplit('/')
        .next()
        .expect("tool entrypoint must have a basename");
    Some(format!(
        "/usr/libexec/safelibs/fallback/{}/{}.real",
        tool.package, leaf
    ))
}

pub fn render_wrapper_script(tool: &RequiredTool) -> Option<String> {
    let fallback = fallback_asset_path(tool)?;
    Some(format!(
        "#!/bin/sh\nset -eu\nSAFE_REAL=\"{fallback}\"\nif [ ! -e \"$SAFE_REAL\" ]; then\n    echo \"missing fallback payload: $SAFE_REAL\" >&2\n    exit 127\nfi\nexec \"$SAFE_REAL\" \"$@\"\n"
    ))
}
