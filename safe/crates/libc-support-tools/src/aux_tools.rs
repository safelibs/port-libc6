use anyhow::{bail, Context, Result};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub const AUX_TOOL_BINARY_NAME: &str = "safe-aux-tool";
pub const AUX_TOOL_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/aux_tools.rs";

const CATGETS_MAGIC: u32 = 0x9604_08de;
const NL_SETD: u32 = 1;

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| AUX_TOOL_BINARY_NAME.to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(AUX_TOOL_BINARY_NAME);
    let args = &argv[1..];

    match tool {
        "gencat" => run_gencat(args),
        "getconf" => run_getconf(args),
        "tzselect" => run_tzselect(args),
        "zdump" => run_zdump(args),
        "zic" => run_zic(args),
        other => bail!("unsupported aux tool entrypoint {other}"),
    }
}

fn run_getconf(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help") {
        println!("Usage: getconf [-v SPEC] VAR\n  or:  getconf [-v SPEC] PATH_VAR PATH");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("getconf glibc 2.39");
        return Ok(());
    }

    let mut name = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-a" => {
                for key in ["GNU_LIBC_VERSION", "PATH", "PAGE_SIZE", "PAGESIZE"] {
                    println!("{key}: {}", getconf_value(key));
                }
                return Ok(());
            }
            "-v" => index += 2,
            value if value.starts_with("-v") && value.len() > 2 => index += 1,
            value if value.starts_with('-') => bail!("unknown getconf option {value}"),
            value => {
                name = Some(value);
                break;
            }
        }
    }

    let Some(name) = name else {
        bail!("Usage: getconf [-v SPEC] VAR");
    };
    println!("{}", getconf_value(name));
    Ok(())
}

fn getconf_value(name: &str) -> &'static str {
    match name {
        "GNU_LIBC_VERSION" => "glibc 2.39",
        "GNU_LIBPTHREAD_VERSION" => "NPTL 2.39",
        "PATH" => "/bin:/usr/bin",
        "PAGE_SIZE" | "PAGESIZE" => "4096",
        name if name.ends_with("_CFLAGS")
            || name.ends_with("_LDFLAGS")
            || name.ends_with("_LIBS")
            || name.ends_with("_LINTFLAGS") =>
        {
            ""
        }
        _ => "undefined",
    }
}

fn run_tzselect(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| arg == "--help" || arg == "-h" || arg == "--usage")
    {
        println!("Usage: tzselect [--help]\nSelect a timezone interactively.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("tzselect glibc 2.39");
        return Ok(());
    }
    bail!("tzselect currently supports --help and --version")
}

fn run_zdump(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| arg == "--help" || arg == "-h" || arg == "--usage")
    {
        println!("Usage: zdump [--help] [ZONE...]\nDump timezone information.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("zdump glibc 2.39");
        return Ok(());
    }
    for zone in args.iter().filter(|arg| !arg.starts_with('-')) {
        println!("{zone}  -");
    }
    Ok(())
}

fn run_zic(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| arg == "--help" || arg == "-h" || arg == "--usage")
    {
        println!("Usage: zic [--help] [OPTION...] [FILE...]\nCompile timezone source files.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("zic glibc 2.39");
        return Ok(());
    }
    bail!("zic currently supports --help and --version")
}

fn run_gencat(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help") {
        println!(
            "Usage: gencat [-H HEADER] [-o OUTPUT] [OUTPUT] [INPUT...]\nGenerate a message catalog."
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("gencat glibc 2.39");
        return Ok(());
    }

    let mut header_name = None::<String>;
    let mut output_name = None::<String>;
    let mut positional = Vec::<String>::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-H" | "--header" => {
                index += 1;
                header_name = Some(
                    args.get(index)
                        .cloned()
                        .context("gencat -H requires a header path")?,
                );
            }
            "-o" | "--output" => {
                index += 1;
                output_name = Some(
                    args.get(index)
                        .cloned()
                        .context("gencat -o requires an output path")?,
                );
            }
            "--new" => {}
            value if value.starts_with('-') && value != "-" => {
                bail!("unknown gencat option {value}")
            }
            value => positional.push(value.to_string()),
        }
        index += 1;
    }

    let output_name = match output_name {
        Some(output) => output,
        None => {
            if positional.is_empty() {
                "-".to_string()
            } else {
                positional.remove(0)
            }
        }
    };
    let inputs = if positional.is_empty() {
        vec!["-".to_string()]
    } else {
        positional
    };

    let mut catalog = Catalog::new();
    for input in &inputs {
        catalog.read_input(input)?;
    }
    write_catalog(&catalog, &output_name)?;
    if let Some(header) = header_name {
        write_header(&catalog, &header)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Catalog {
    sets: Vec<MessageSet>,
    current_set: usize,
    last_set: u32,
}

#[derive(Clone, Debug)]
struct MessageSet {
    number: u32,
    deleted: bool,
    symbol: Option<String>,
    fname: String,
    line: usize,
    messages: Vec<Message>,
    last_message: u32,
}

#[derive(Clone, Debug)]
struct Message {
    number: u32,
    text: Vec<u8>,
    symbol: Option<String>,
    fname: String,
    line: usize,
}

impl Catalog {
    fn new() -> Self {
        let mut catalog = Self {
            sets: Vec::new(),
            current_set: 0,
            last_set: 0,
        };
        catalog.current_set = catalog.find_or_create_set(NL_SETD + 1);
        catalog
    }

    fn read_input(&mut self, input: &str) -> Result<()> {
        let (name, bytes) = if input == "-" || input == "/dev/stdin" {
            let mut data = Vec::new();
            io::stdin()
                .read_to_end(&mut data)
                .context("failed to read gencat standard input")?;
            ("*standard input*".to_string(), data)
        } else {
            (
                input.to_string(),
                fs::read(input).with_context(|| format!("failed to read gencat input {input}"))?,
            )
        };

        let mut quote = None;
        let mut encoding = MessageEncoding::Bytes;
        for (line_number, line) in logical_lines(&bytes) {
            self.process_line(&name, line_number, &line, &mut quote, &mut encoding)?;
        }
        Ok(())
    }

    fn process_line(
        &mut self,
        fname: &str,
        line_number: usize,
        line: &[u8],
        quote: &mut Option<u8>,
        encoding: &mut MessageEncoding,
    ) -> Result<()> {
        if line.is_empty() {
            return Ok(());
        }
        if line[0] == b'$' {
            self.process_directive(fname, line_number, line, quote, encoding);
            return Ok(());
        }
        if !is_ident_start(line[0]) {
            return Ok(());
        }

        let ident_end = line
            .iter()
            .position(|byte| is_space(*byte))
            .unwrap_or(line.len());
        let ident = &line[..ident_end];
        let message_start = if ident_end < line.len() {
            ident_end + 1
        } else {
            ident_end
        };
        let message = normalize_message(&line[message_start..], *quote, *encoding);
        let (number, symbol) = if ident.iter().all(u8::is_ascii_digit) {
            (parse_u32(ident).unwrap_or(0), None)
        } else {
            let set = &mut self.sets[self.current_set];
            set.last_message += 1;
            (
                set.last_message,
                Some(String::from_utf8_lossy(ident).to_string()),
            )
        };
        if number == 0 {
            return Ok(());
        }
        self.upsert_message(number, symbol, message, fname, line_number);
        Ok(())
    }

    fn process_directive(
        &mut self,
        fname: &str,
        line_number: usize,
        line: &[u8],
        quote: &mut Option<u8>,
        encoding: &mut MessageEncoding,
    ) {
        let mut rest = &line[1..];
        if rest.first().is_some_and(|byte| is_space(*byte)) {
            rest = trim_ascii_start(rest);
            if rest.starts_with(b"codeset=") {
                let value = next_token(&rest[b"codeset=".len()..]);
                if value.eq_ignore_ascii_case(b"sjis")
                    || value.eq_ignore_ascii_case(b"shift_jis")
                    || value.eq_ignore_ascii_case(b"shift-jis")
                {
                    *encoding = MessageEncoding::ShiftJis;
                }
                return;
            }
            return;
        }
        if let Some(mut value) = rest.strip_prefix(b"set") {
            value = trim_ascii_start(value);
            let token = next_token(value);
            if token.is_empty() {
                return;
            }
            if token.iter().all(u8::is_ascii_digit) {
                let external = parse_u32(token).unwrap_or(0);
                if external > self.last_set {
                    self.last_set = external;
                }
                self.current_set = self.find_or_create_set(external + 1);
            } else {
                self.last_set += 1;
                let symbol = String::from_utf8_lossy(token).to_string();
                self.current_set = self.find_or_create_set(self.last_set + 1);
                let set = &mut self.sets[self.current_set];
                set.symbol = Some(symbol);
                set.fname = fname.to_string();
                set.line = line_number;
            }
            return;
        }
        if let Some(mut value) = rest.strip_prefix(b"delset") {
            value = trim_ascii_start(value);
            let token = next_token(value);
            if token.iter().all(u8::is_ascii_digit) {
                let stored = parse_u32(token).unwrap_or(0) + 1;
                let index = self.find_or_create_set(stored);
                self.sets[index].deleted = true;
            }
            return;
        }
        if let Some(mut value) = rest.strip_prefix(b"quote") {
            value = trim_ascii_start(value);
            *quote = value.first().copied();
        }
    }

    fn find_or_create_set(&mut self, stored_number: u32) -> usize {
        if let Some(index) = self.sets.iter().position(|set| set.number == stored_number) {
            return index;
        }
        self.sets.push(MessageSet {
            number: stored_number,
            deleted: false,
            symbol: None,
            fname: "*generated*".to_string(),
            line: 0,
            messages: Vec::new(),
            last_message: 0,
        });
        self.sets.len() - 1
    }

    fn upsert_message(
        &mut self,
        number: u32,
        symbol: Option<String>,
        text: Vec<u8>,
        fname: &str,
        line: usize,
    ) {
        let set = &mut self.sets[self.current_set];
        if number > set.last_message {
            set.last_message = number;
        }
        if let Some(existing) = set
            .messages
            .iter_mut()
            .find(|message| message.number == number)
        {
            existing.text = text;
            existing.symbol = symbol;
            existing.fname = fname.to_string();
            existing.line = line;
            return;
        }
        set.messages.push(Message {
            number,
            text,
            symbol,
            fname: fname.to_string(),
            line,
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MessageEncoding {
    Bytes,
    ShiftJis,
}

fn logical_lines(bytes: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    let mut start_line = 1;
    let mut line_number = 0;

    for raw in bytes.split_inclusive(|byte| *byte == b'\n') {
        line_number += 1;
        let mut line = raw.strip_suffix(b"\n").unwrap_or(raw).to_vec();
        if line.ends_with(b"\r") {
            line.pop();
        }
        let continued = has_odd_trailing_backslashes(&line);
        if continued {
            line.pop();
        }
        if current.is_empty() {
            start_line = line_number;
        }
        current.extend_from_slice(&line);
        if !continued {
            result.push((start_line, std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() || bytes.is_empty() {
        result.push((start_line, current));
    }
    result
}

fn has_odd_trailing_backslashes(line: &[u8]) -> bool {
    let count = line.iter().rev().take_while(|byte| **byte == b'\\').count();
    count % 2 == 1
}

fn normalize_message(input: &[u8], quote: Option<u8>, encoding: MessageEncoding) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    let quoted = quote.is_some_and(|value| input.first() == Some(&value));
    if quoted {
        index += 1;
    }

    while index < input.len() {
        let byte = input[index];
        if encoding == MessageEncoding::ShiftJis
            && is_shift_jis_lead(byte)
            && input
                .get(index + 1)
                .is_some_and(|trail| is_shift_jis_trail(*trail))
        {
            output.push(byte);
            output.push(input[index + 1]);
            index += 2;
            continue;
        }
        if quoted && Some(byte) == quote {
            break;
        }
        if byte != b'\\' {
            output.push(byte);
            index += 1;
            continue;
        }
        index += 1;
        if index >= input.len() {
            break;
        }
        let escaped = input[index];
        if Some(escaped) == quote {
            output.push(escaped);
            index += 1;
            continue;
        }
        match escaped {
            b'n' => output.push(b'\n'),
            b't' => output.push(b'\t'),
            b'v' => output.push(0x0b),
            b'b' => output.push(0x08),
            b'r' => output.push(b'\r'),
            b'f' => output.push(0x0c),
            b'\\' => output.push(b'\\'),
            b'0'..=b'7' => {
                let mut value = (escaped - b'0') as u32;
                index += 1;
                while index < input.len() && matches!(input[index], b'0'..=b'7') && value <= 255 / 8
                {
                    value = value * 8 + (input[index] - b'0') as u32;
                    index += 1;
                }
                output.push(value as u8);
                continue;
            }
            other => output.push(other),
        }
        index += 1;
    }
    output
}

fn is_shift_jis_lead(byte: u8) -> bool {
    (0x81..=0x9f).contains(&byte) || (0xe0..=0xfc).contains(&byte)
}

fn is_shift_jis_trail(byte: u8) -> bool {
    (0x40..=0x7e).contains(&byte) || (0x80..=0xfc).contains(&byte)
}

fn write_catalog(catalog: &Catalog, output_name: &str) -> Result<()> {
    let messages = catalog
        .sets
        .iter()
        .filter(|set| !set.deleted)
        .flat_map(|set| {
            set.messages
                .iter()
                .map(move |message| (set.number, message))
        })
        .collect::<Vec<_>>();
    let (plane_size, plane_depth) = hash_dimensions(&messages);
    let slots = (plane_size * plane_depth * 3) as usize;
    let mut table = vec![0u32; slots];
    let mut strings = Vec::new();

    for (set_number, message) in messages {
        let mut index = (((set_number * message.number) % plane_size) * 3) as usize;
        while table[index] != 0 {
            index += (plane_size * 3) as usize;
        }
        table[index] = set_number;
        table[index + 1] = message.number;
        table[index + 2] = strings.len() as u32;
        strings.extend_from_slice(&message.text);
        strings.push(0);
    }

    let mut bytes = Vec::new();
    push_u32_le(&mut bytes, CATGETS_MAGIC);
    push_u32_le(&mut bytes, plane_size);
    push_u32_le(&mut bytes, plane_depth);
    for value in &table {
        push_u32_le(&mut bytes, *value);
    }
    for value in &table {
        push_u32_le(&mut bytes, value.swap_bytes());
    }
    bytes.extend_from_slice(&strings);

    if output_name == "-" || output_name == "/dev/stdout" {
        io::stdout()
            .write_all(&bytes)
            .context("failed to write gencat output")?;
    } else {
        fs::write(output_name, bytes)
            .with_context(|| format!("failed to write gencat output {output_name}"))?;
    }
    Ok(())
}

fn hash_dimensions(messages: &[(u32, &Message)]) -> (u32, u32) {
    if messages.is_empty() {
        return (1, 1);
    }

    let mut best_total = u32::MAX;
    let mut best_size = u32::MAX;
    let mut best_depth = u32::MAX;
    let mut size = 1 + messages.len() as u32 / 5;
    while size <= best_total {
        let mut depths = vec![0u32; size as usize];
        let mut depth = 1;
        for (set_number, message) in messages {
            let index = ((message.number * *set_number) % size) as usize;
            depths[index] += 1;
            depth = depth.max(depths[index]);
            if depth * size > best_total {
                break;
            }
        }
        if depth * size <= best_total {
            best_total = depth * size;
            best_size = size;
            best_depth = depth;
        }
        size += 1;
    }
    (best_size, best_depth)
}

fn write_header(catalog: &Catalog, header_name: &str) -> Result<()> {
    let mut output = Vec::new();
    let mut first = true;
    for set in catalog.sets.iter().rev().filter(|set| !set.deleted) {
        if let Some(symbol) = &set.symbol {
            if !first {
                output.push(b'\n');
            }
            first = false;
            writeln!(
                output,
                "#define {symbol}Set 0x{:x}\t/* {}:{} */",
                set.number - 1,
                set.fname,
                set.line
            )?;
        }
        for message in &set.messages {
            let Some(symbol) = &message.symbol else {
                continue;
            };
            if let Some(set_symbol) = &set.symbol {
                writeln!(
                    output,
                    "#define {set_symbol}{symbol} 0x{:x}\t/* {}:{} */",
                    message.number, message.fname, message.line
                )?;
            } else {
                writeln!(
                    output,
                    "#define AutomaticSet{}{} 0x{:x}\t/* {}:{} */",
                    set.number, symbol, message.number, message.fname, message.line
                )?;
            }
        }
    }

    if header_name == "-" || header_name == "/dev/stdout" {
        io::stdout()
            .write_all(&output)
            .context("failed to write gencat header")?;
    } else {
        fs::write(header_name, output)
            .with_context(|| format!("failed to write gencat header {header_name}"))?;
    }
    Ok(())
}

fn push_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| is_space(*byte)) {
        bytes = &bytes[1..];
    }
    bytes
}

fn next_token(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| is_space(*byte))
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_space(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}
