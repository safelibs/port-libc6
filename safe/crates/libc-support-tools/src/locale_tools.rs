use anyhow::{anyhow, bail, Context, Result};
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

    let converted = convert_bytes(&input_bytes, &from, &to, omit_invalid)?;
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

fn convert_bytes(input: &[u8], from: &str, to: &str, omit_invalid: bool) -> Result<Vec<u8>> {
    let from = normalize_encoding(from);
    let to = normalize_encoding(to);
    if from == to || is_utf8_alias(&from) && is_utf8_alias(&to) {
        return Ok(input.to_vec());
    }

    let text = decode_to_string(input, &from, omit_invalid)?;
    encode_from_string(&text, &to, omit_invalid)
}

fn normalize_encoding(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('/')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

fn is_utf8_alias(value: &str) -> bool {
    value == "UTF8"
}

fn decode_to_string(input: &[u8], encoding: &str, omit_invalid: bool) -> Result<String> {
    match encoding {
        "UTF8" => {
            if omit_invalid {
                Ok(String::from_utf8_lossy(input).into_owned())
            } else {
                String::from_utf8(input.to_vec()).context("invalid UTF-8 input")
            }
        }
        "ASCII" | "ANSIX341968" => {
            let mut out = String::with_capacity(input.len());
            for byte in input {
                if byte.is_ascii() {
                    out.push(*byte as char);
                } else if !omit_invalid {
                    bail!("invalid ASCII input byte 0x{byte:02x}");
                }
            }
            Ok(out)
        }
        "ISO88591" | "LATIN1" => Ok(input.iter().map(|byte| *byte as char).collect()),
        "UTF16LE" | "UTF16BE" => decode_utf16(input, encoding == "UTF16LE", omit_invalid),
        _ => Ok(String::from_utf8_lossy(input).into_owned()),
    }
}

fn decode_utf16(input: &[u8], little_endian: bool, omit_invalid: bool) -> Result<String> {
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
        bail!("incomplete UTF-16 input unit");
    }

    let mut out = String::new();
    for item in char::decode_utf16(words) {
        match item {
            Ok(ch) => out.push(ch),
            Err(_) if omit_invalid => {}
            Err(_) => bail!("invalid UTF-16 input sequence"),
        }
    }
    Ok(out)
}

fn encode_from_string(text: &str, encoding: &str, omit_invalid: bool) -> Result<Vec<u8>> {
    match encoding {
        "UTF8" => Ok(text.as_bytes().to_vec()),
        "ASCII" | "ANSIX341968" => {
            let mut out = Vec::with_capacity(text.len());
            for ch in text.chars() {
                if ch.is_ascii() {
                    out.push(ch as u8);
                } else if !omit_invalid {
                    bail!(
                        "character U+{:04X} cannot be represented as ASCII",
                        ch as u32
                    );
                }
            }
            Ok(out)
        }
        "ISO88591" | "LATIN1" => {
            let mut out = Vec::with_capacity(text.len());
            for ch in text.chars() {
                let code = ch as u32;
                if code <= 0xff {
                    out.push(code as u8);
                } else if !omit_invalid {
                    bail!(
                        "character U+{:04X} cannot be represented as ISO-8859-1",
                        code
                    );
                }
            }
            Ok(out)
        }
        "UTF16LE" | "UTF16BE" => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for word in text.encode_utf16() {
                let bytes = if encoding == "UTF16LE" {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                };
                out.extend(bytes);
            }
            Ok(out)
        }
        _ => Ok(text.as_bytes().to_vec()),
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
    let lang = env::var("LANG").unwrap_or_else(|_| "C.UTF-8".to_string());
    println!("LANG={lang}");
    for key in [
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
    ] {
        println!("{key}=\"{}\"", env::var(key).unwrap_or_default());
    }
    println!("LC_ALL=\"{}\"", env::var("LC_ALL").unwrap_or_default());
}

fn current_locale() -> String {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    "C.UTF-8".to_string()
}

fn current_charmap() -> &'static str {
    let locale = current_locale().to_ascii_uppercase();
    if locale == "C" || locale == "POSIX" {
        "ANSI_X3.4-1968"
    } else if locale.contains("UTF-8") || locale.contains("UTF8") {
        "UTF-8"
    } else {
        "ISO-8859-1"
    }
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

fn normalize_locale_name(value: &str) -> String {
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
