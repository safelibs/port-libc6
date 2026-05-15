use anyhow::{anyhow, bail, Context, Result};
use libc6::iconv::{convert_bytes as libc_convert_bytes, ConversionOptions};
use libc6::locale::{
    category_keys, charmap_for_locale, current_locale_from_pairs, locale_environment_from_pairs,
    normalize_locale_name,
};
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const LOCALE_TOOL_BINARY_NAME: &str = "safe-locale-tool";
pub const LOCALE_TOOL_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/locale_tools.rs";

const LOCALE_ARCHIVE_REGISTRY: &str = "/usr/lib/locale/safelibs-locale-archive";
const GCONV_CACHE: &str = "/usr/lib/gconv/gconv-modules.cache";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| LOCALE_TOOL_BINARY_NAME.to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(LOCALE_TOOL_BINARY_NAME);

    match tool {
        "iconv" => iconv_main(&argv[1..]),
        "iconvconfig" => iconvconfig_main(&argv[1..]),
        "locale" => locale_main(&argv[1..]),
        "localedef" => localedef_main(&argv[1..]),
        other => bail!("unsupported locale tool entrypoint {other}"),
    }
}

fn iconv_main(args: &[String]) -> Result<()> {
    let mut from = String::from("UTF-8");
    let mut to = String::from("UTF-8");
    let mut output: Option<PathBuf> = None;
    let mut omit_invalid = false;
    let mut inputs = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--help" | "-h" => {
                print_iconv_help();
                return Ok(());
            }
            "--version" => {
                println!("iconv (safelibs) 0.1");
                return Ok(());
            }
            "-l" | "--list" => {
                print_supported_encodings();
                return Ok(());
            }
            "-c" => omit_invalid = true,
            "-s" => {}
            "-f" | "--from-code" => {
                index += 1;
                from = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| anyhow!("{arg} requires an argument"))?;
            }
            "-t" | "--to-code" => {
                index += 1;
                to = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| anyhow!("{arg} requires an argument"))?;
            }
            "-o" | "--output" => {
                index += 1;
                output = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("{arg} requires an argument"))?,
                ));
            }
            _ if arg.starts_with("--from-code=") => {
                from = arg["--from-code=".len()..].to_string();
            }
            _ if arg.starts_with("--to-code=") => {
                to = arg["--to-code=".len()..].to_string();
            }
            _ if arg.starts_with("--output=") => {
                output = Some(PathBuf::from(&arg["--output=".len()..]));
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                bail!("iconv: unrecognized option {arg}");
            }
            _ => inputs.push(arg.clone()),
        }
        index += 1;
    }

    let mut input_bytes = Vec::new();
    if inputs.is_empty() {
        io::stdin()
            .read_to_end(&mut input_bytes)
            .context("failed to read stdin")?;
    } else {
        for path in inputs {
            input_bytes.extend(
                fs::read(&path).with_context(|| format!("failed to read iconv input {path}"))?,
            );
        }
    }

    let converted =
        libc_convert_bytes(&input_bytes, &from, &to, ConversionOptions { omit_invalid })?;
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        fs::write(&path, converted).with_context(|| format!("failed to write {}", path.display()))
    } else {
        io::stdout()
            .write_all(&converted)
            .context("failed to write iconv output")
    }
}

fn print_iconv_help() {
    println!("Usage: iconv [OPTION...] [FILE...]");
    println!("Convert text from one character encoding to another.");
    println!("  -f, --from-code=NAME  encoding of original text");
    println!("  -t, --to-code=NAME    encoding for output");
    println!("  -o, --output=FILE     write output to FILE");
    println!("  -l, --list            list supported encodings");
}

fn print_supported_encodings() {
    for name in [
        "ANSI_X3.4-1968//",
        "ASCII//",
        "ISO-8859-1//",
        "LATIN1//",
        "UTF-8//",
        "UTF8//",
        "UTF-16BE//",
        "UTF-16LE//",
    ] {
        println!("{name}");
    }
}

fn iconvconfig_main(args: &[String]) -> Result<()> {
    let mut output = PathBuf::from(GCONV_CACHE);
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => {
                println!("Usage: iconvconfig [OPTION...]");
                println!("Create a gconv module cache for the installed locale payload.");
                println!("  -o, --output=FILE  write cache to FILE");
                return Ok(());
            }
            "--version" => {
                println!("iconvconfig (safelibs) 0.1");
                return Ok(());
            }
            "-o" | "--output" => {
                index += 1;
                output = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("{} requires an argument", args[index - 1]))?,
                );
            }
            arg if arg.starts_with("--output=") => {
                output = PathBuf::from(&arg["--output=".len()..]);
            }
            arg if arg.starts_with('-') => bail!("iconvconfig: unrecognized option {arg}"),
            _ => {}
        }
        index += 1;
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&output, b"# safelibs gconv cache placeholder\n")
        .with_context(|| format!("failed to write {}", output.display()))
}

fn locale_main(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: locale [OPTION...] [NAME]");
        println!("  -a, --all-locales  write names of available locales");
        println!("  -m, --charmaps     write names of available charmaps");
        println!("      charmap        write the active character map");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("locale (safelibs) 0.1");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "-a" || arg == "--all-locales") {
        for locale in available_locales()? {
            println!("{locale}");
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "-m" || arg == "--charmaps") {
        for charmap in available_charmaps()? {
            println!("{charmap}");
        }
        return Ok(());
    }
    if args.len() == 1 && args[0] == "charmap" {
        println!("{}", current_charmap());
        return Ok(());
    }
    if args.is_empty() {
        print_locale_environment();
        return Ok(());
    }

    for arg in args {
        match arg.as_str() {
            "charmap" => println!("{}", current_charmap()),
            "language" => println!("{}", current_locale()),
            name if name.starts_with("LC_") || name == "LANG" => {
                println!(
                    "{}=\"{}\"",
                    name,
                    env::var(name).unwrap_or_else(|_| current_locale())
                );
            }
            other => bail!("locale: unsupported name {other}"),
        }
    }
    Ok(())
}

fn print_locale_environment() {
    let locale_env = locale_environment_from_pairs(env::vars());
    println!("LANG={}", locale_env.lang);
    for (key, value) in locale_env.categories {
        println!("{key}=\"{value}\"");
    }
    println!("LC_ALL=\"{}\"", locale_env.lc_all);
}

fn current_locale() -> String {
    current_locale_from_pairs(env::vars())
}

fn current_charmap() -> &'static str {
    charmap_for_locale(&current_locale())
}

fn available_locales() -> Result<Vec<String>> {
    let mut locales = BTreeSet::new();
    locales.insert("C".to_string());
    locales.insert("C.utf8".to_string());
    locales.insert("POSIX".to_string());

    for path in locale_registry_paths() {
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                locales.insert(normalize_locale_name(line));
            }
        }
    }

    let locale_root = Path::new("/usr/lib/locale");
    if let Ok(entries) = fs::read_dir(locale_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("LC_CTYPE").exists() {
                if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                    locales.insert(normalize_locale_name(name));
                }
            }
        }
    }

    Ok(locales.into_iter().collect())
}

fn available_charmaps() -> Result<Vec<String>> {
    let mut names = BTreeSet::new();
    for builtin in ["ANSI_X3.4-1968", "ISO-8859-1", "UTF-8"] {
        names.insert(builtin.to_string());
    }
    for key in category_keys() {
        if let Ok(value) = env::var(key) {
            if !value.is_empty() {
                names.insert(charmap_for_locale(&value).to_string());
            }
        }
    }
    if let Ok(entries) = fs::read_dir("/usr/share/i18n/charmaps") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                names.insert(name.trim_end_matches(".gz").to_string());
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn localedef_main(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: localedef [OPTION...] NAME");
        println!("Compile locale source definitions into the installed locale store.");
        println!("  --list-archive          list locales in the archive registry");
        println!("  --delete-from-archive   remove locales from the archive registry");
        println!("  --no-archive            also accepted for Debian locale-gen compatibility");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("localedef (safelibs) 0.1");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--list-archive") {
        for locale in available_locales()? {
            if locale != "C" && locale != "POSIX" {
                println!("{locale}");
            }
        }
        return Ok(());
    }

    if let Some(pos) = args.iter().position(|arg| arg == "--delete-from-archive") {
        let names = args[pos + 1..]
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .map(|arg| arg.as_str())
            .collect::<Vec<_>>();
        remove_locales_from_registry(&names)?;
        return Ok(());
    }

    let mut input = String::new();
    let mut charmap = String::from("UTF-8");
    let mut output_name: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-i" | "--inputfile" => {
                index += 1;
                input = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| anyhow!("{} requires an argument", args[index - 1]))?;
            }
            "-f" | "--charmap" => {
                index += 1;
                charmap = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| anyhow!("{} requires an argument", args[index - 1]))?;
            }
            "-A" | "--alias-file" => {
                index += 1;
                let _ = args
                    .get(index)
                    .ok_or_else(|| anyhow!("{} requires an argument", args[index - 1]))?;
            }
            "-c" | "--force" | "--no-archive" | "--posix" | "--quiet" | "--verbose" => {}
            arg if arg.starts_with("--prefix=")
                || arg.starts_with("--repertoire-map=")
                || arg.starts_with("--alias-file=")
                || arg.starts_with("--inputfile=")
                || arg.starts_with("--charmap=") => {}
            arg if arg.starts_with('-') => {}
            value => output_name = Some(value.to_string()),
        }
        index += 1;
    }

    let Some(output_name) = output_name else {
        bail!("localedef: missing output locale name");
    };
    register_locale(&output_name, &input, &charmap)
}

fn register_locale(name: &str, input: &str, charmap: &str) -> Result<()> {
    let normalized = normalize_locale_name(name);
    let locale_dir = Path::new("/usr/lib/locale").join(&normalized);
    fs::create_dir_all(&locale_dir)
        .with_context(|| format!("failed to create {}", locale_dir.display()))?;
    write_marker(locale_dir.join("LC_CTYPE"), &normalized, input, charmap)?;
    write_marker(
        locale_dir.join("LC_IDENTIFICATION"),
        &normalized,
        input,
        charmap,
    )?;

    let registry = PathBuf::from(LOCALE_ARCHIVE_REGISTRY);
    if let Some(parent) = registry.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut locales = BTreeSet::new();
    if let Ok(text) = fs::read_to_string(&registry) {
        locales.extend(text.lines().map(normalize_locale_name));
    }
    locales.insert(normalized);
    let mut file = File::create(&registry)
        .with_context(|| format!("failed to write {}", registry.display()))?;
    for locale in locales {
        writeln!(file, "{locale}")?;
    }
    Ok(())
}

fn write_marker(path: PathBuf, locale: &str, input: &str, charmap: &str) -> Result<()> {
    fs::write(
        &path,
        format!("safelibs-locale\nlocale={locale}\ninput={input}\ncharmap={charmap}\n"),
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn remove_locales_from_registry(names: &[&str]) -> Result<()> {
    let remove = names
        .iter()
        .map(|name| normalize_locale_name(name))
        .collect::<BTreeSet<_>>();
    let registry = PathBuf::from(LOCALE_ARCHIVE_REGISTRY);
    let mut keep = BTreeSet::new();
    if let Ok(text) = fs::read_to_string(&registry) {
        for line in text.lines() {
            let normalized = normalize_locale_name(line);
            if !remove.contains(&normalized) {
                keep.insert(normalized);
            }
        }
    }
    if let Some(parent) = registry.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = File::create(&registry)
        .with_context(|| format!("failed to write {}", registry.display()))?;
    for locale in keep {
        writeln!(file, "{locale}")?;
    }
    Ok(())
}

fn locale_registry_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from(LOCALE_ARCHIVE_REGISTRY),
        PathBuf::from("/usr/lib/locale/locale-archive"),
    ]
}
