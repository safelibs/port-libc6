use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub const NETWORK_TOOL_BINARY_NAME: &str = "safe-network-tool";
pub const GETENT_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/network_tools.rs";
pub const NSCD_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/network_tools.rs";
const NSCD_RUN_DIR: &str = "/run/nscd";
const NSCD_CACHE_DIR: &str = "/var/cache/nscd";
const NSCD_PIDFILE: &str = "/run/nscd/nscd.pid";
const NSCD_SOCKET_MARKER: &str = "/run/nscd/socket";
const NSCD_INVALIDATION_DIR: &str = "/run/nscd/invalidations";
const NSCD_SNAPSHOT: &str = "/run/nscd/state.snapshot";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let program = argv
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(OsStr::to_str)
        .unwrap_or(NETWORK_TOOL_BINARY_NAME);
    match program {
        "getent" => run_getent(&argv[1..]),
        "nscd" => run_nscd(&argv[1..]),
        _ => bail!(
            "unknown network-tool invocation {program}; install it as /usr/bin/getent or /usr/sbin/nscd"
        ),
    }
}

fn run_getent(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_getent_help();
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("getent safelibs-network-tool");
        return Ok(());
    }

    let filtered = args
        .iter()
        .filter(|arg| arg.as_str() != "--no-addrconfig")
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        bail!("getent requires a database name");
    }
    let database = filtered[0].as_str();
    let keys = &filtered[1..];

    match database {
        "passwd" => query_keyed_file("/etc/passwd", keys, 0),
        "group" => query_keyed_file("/etc/group", keys, 0),
        "services" => query_services(keys),
        "protocols" => query_keyed_file("/etc/protocols", keys, 0),
        "hosts" => query_hosts(keys, HostFamily::Any, false),
        "ahosts" => query_hosts(keys, HostFamily::Any, true),
        "ahostsv4" => query_hosts(keys, HostFamily::V4, true),
        "ahostsv6" => query_hosts(keys, HostFamily::V6, true),
        other => bail!("unsupported getent database {other}"),
    }
}

fn print_getent_help() {
    println!("usage: getent [--no-addrconfig] database key ...");
    println!("supported databases: passwd group hosts ahosts ahostsv4 ahostsv6 services protocols");
}

fn query_keyed_file(path: &str, keys: &[String], key_field: usize) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read database {path}"))?;
    let mut lines = contents
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    if keys.is_empty() {
        for line in lines {
            println!("{line}");
        }
        return Ok(());
    }

    lines.retain(|line| {
        let fields = line.split(':').collect::<Vec<_>>();
        let key = fields.get(key_field).copied().unwrap_or_default();
        keys.iter().any(|candidate| candidate == key)
    });
    if lines.is_empty() {
        std::process::exit(2);
    }
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

fn query_services(keys: &[String]) -> Result<()> {
    let contents =
        fs::read_to_string("/etc/services").context("failed to read database /etc/services")?;
    let mut matches = Vec::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if keys.is_empty() {
            matches.push(line.to_string());
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let name = fields.first().copied().unwrap_or_default();
        let port = fields.get(1).copied().unwrap_or_default();
        if keys
            .iter()
            .any(|candidate| candidate == name || candidate == port)
        {
            matches.push(line.to_string());
        }
    }
    if matches.is_empty() {
        std::process::exit(2);
    }
    for line in matches {
        println!("{line}");
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum HostFamily {
    Any,
    V4,
    V6,
}

fn query_hosts(keys: &[String], family: HostFamily, expanded: bool) -> Result<()> {
    if keys.is_empty() {
        bail!("host queries require at least one key");
    }

    let mut rows = Vec::new();
    for key in keys {
        if let Some(ip) = parse_numeric_host(key) {
            if matches_family(ip, family) {
                append_host_rows(&mut rows, ip, key, expanded);
            }
            continue;
        }
        let mut seen = BTreeSet::new();
        let addrs = (key.as_str(), 0)
            .to_socket_addrs()
            .with_context(|| format!("failed to resolve host {key}"))?;
        for addr in addrs {
            let ip = addr.ip();
            if !matches_family(ip, family) || !seen.insert(ip) {
                continue;
            }
            append_host_rows(&mut rows, ip, key, expanded);
        }
    }

    if rows.is_empty() {
        std::process::exit(2);
    }
    for row in rows {
        println!("{row}");
    }
    Ok(())
}

fn parse_numeric_host(key: &str) -> Option<IpAddr> {
    if key.is_empty() || key.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    key.parse::<IpAddr>().ok()
}

fn append_host_rows(rows: &mut Vec<String>, ip: IpAddr, key: &str, expanded: bool) {
    if expanded {
        rows.push(format!("{ip} STREAM {key}"));
        rows.push(format!("{ip} DGRAM"));
        rows.push(format!("{ip} RAW"));
    } else {
        rows.push(format!("{ip} {key}"));
    }
}

fn matches_family(ip: IpAddr, family: HostFamily) -> bool {
    match family {
        HostFamily::Any => true,
        HostFamily::V4 => ip.is_ipv4(),
        HostFamily::V6 => ip.is_ipv6(),
    }
}

fn run_nscd(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_nscd_help();
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("nscd safelibs-network-tool");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--shutdown" || arg == "-K") {
        shutdown_nscd()?;
        return Ok(());
    }
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "-i" || arg == "--invalidate")
    {
        let database = args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("nscd -i requires a database name"))?;
        invalidate_database(database)?;
        return Ok(());
    }
    if args.iter().any(|arg| arg == "-g") {
        print_nscd_status()?;
        return Ok(());
    }

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "-d" | "--foreground" => {
                index += 1;
            }
            "-f" => {
                index += 2;
            }
            other if other.starts_with('-') => bail!("unsupported nscd option {other}"),
            _ => {
                index += 1;
            }
        }
    }

    run_nscd_loop()
}

fn print_nscd_help() {
    println!("usage: nscd [--help] [--version] [--shutdown] [-i database] [-g]");
    println!("The safe phase-07 frontend provides the control-plane surface used by the packaging harness.");
}

fn run_nscd_loop() -> Result<()> {
    fs::create_dir_all(NSCD_RUN_DIR).with_context(|| format!("failed to create {NSCD_RUN_DIR}"))?;
    fs::create_dir_all(NSCD_CACHE_DIR)
        .with_context(|| format!("failed to create {NSCD_CACHE_DIR}"))?;
    fs::create_dir_all(NSCD_INVALIDATION_DIR)
        .with_context(|| format!("failed to create {NSCD_INVALIDATION_DIR}"))?;

    let pid = std::process::id().to_string();
    fs::write(NSCD_PIDFILE, format!("{pid}\n"))
        .with_context(|| format!("failed to write {NSCD_PIDFILE}"))?;
    fs::write(NSCD_SOCKET_MARKER, "safe nscd socket marker\n")
        .with_context(|| format!("failed to write {NSCD_SOCKET_MARKER}"))?;
    write_nscd_snapshot(1, &pid, &[])?;

    loop {
        thread::sleep(Duration::from_secs(1));
        let Ok(current) = fs::read_to_string(NSCD_PIDFILE) else {
            break;
        };
        if current.trim() != pid {
            break;
        }
        let generation = read_nscd_snapshot()
            .map(|snapshot| snapshot.generation.saturating_add(1))
            .unwrap_or(1);
        write_nscd_snapshot(generation, &pid, &collect_invalidations()?)?;
    }

    let _ = fs::remove_file(NSCD_SOCKET_MARKER);
    let _ = fs::remove_file(NSCD_SNAPSHOT);
    Ok(())
}

fn shutdown_nscd() -> Result<()> {
    if Path::new(NSCD_PIDFILE).exists() {
        fs::remove_file(NSCD_PIDFILE)
            .with_context(|| format!("failed to remove {NSCD_PIDFILE}"))?;
    }
    let _ = fs::remove_file(NSCD_SOCKET_MARKER);
    let _ = fs::remove_file(NSCD_SNAPSHOT);
    Ok(())
}

fn invalidate_database(database: &str) -> Result<()> {
    fs::create_dir_all(NSCD_INVALIDATION_DIR)
        .with_context(|| format!("failed to create {NSCD_INVALIDATION_DIR}"))?;
    let marker = PathBuf::from(NSCD_INVALIDATION_DIR).join(database);
    fs::write(&marker, "invalidated\n")
        .with_context(|| format!("failed to write {}", marker.display()))?;
    if let Ok(snapshot) = read_nscd_snapshot() {
        let mut invalidations = snapshot.invalidations;
        if !invalidations.iter().any(|entry| entry == database) {
            invalidations.push(database.to_string());
        }
        write_nscd_snapshot(
            snapshot.generation.saturating_add(1),
            &snapshot.pid,
            &invalidations,
        )?;
    }
    Ok(())
}

fn print_nscd_status() -> Result<()> {
    let snapshot = read_nscd_snapshot().ok();
    let pid_text = snapshot
        .as_ref()
        .map(|snapshot| snapshot.pid.clone())
        .or_else(|| {
            fs::read_to_string(NSCD_PIDFILE)
                .ok()
                .map(|pid| pid.trim().to_string())
        })
        .unwrap_or_else(|| "stopped".to_string());
    println!("nscd pid: {pid_text}");
    println!("runtime dir: {NSCD_RUN_DIR}");
    println!("cache dir: {NSCD_CACHE_DIR}");
    if let Some(snapshot) = snapshot {
        println!("snapshot generation: {}", snapshot.generation);
        if !snapshot.invalidations.is_empty() {
            println!("invalidated: {}", snapshot.invalidations.join(","));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct NscdSnapshot {
    generation: u64,
    pid: String,
    invalidations: Vec<String>,
}

fn write_nscd_snapshot(generation: u64, pid: &str, invalidations: &[String]) -> Result<()> {
    fs::create_dir_all(NSCD_RUN_DIR).with_context(|| format!("failed to create {NSCD_RUN_DIR}"))?;
    let mut payload = String::new();
    payload.push_str(&format!("generation={generation}\n"));
    payload.push_str(&format!("pid={pid}\n"));
    if !invalidations.is_empty() {
        payload.push_str("invalidations=");
        payload.push_str(&invalidations.join(","));
        payload.push('\n');
    }
    payload.push_str(&format!("generation_end={generation}\n"));
    let tmp_path = format!("{}.{}.tmp", NSCD_SNAPSHOT, std::process::id());
    fs::write(&tmp_path, payload).with_context(|| format!("failed to write {tmp_path}"))?;
    fs::rename(&tmp_path, NSCD_SNAPSHOT)
        .with_context(|| format!("failed to publish {NSCD_SNAPSHOT}"))?;
    Ok(())
}

fn read_nscd_snapshot() -> Result<NscdSnapshot> {
    for _ in 0..3 {
        match fs::read_to_string(NSCD_SNAPSHOT) {
            Ok(contents) => {
                if let Some(snapshot) = parse_nscd_snapshot(&contents) {
                    return Ok(snapshot);
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => bail!("nscd snapshot is absent"),
            Err(error) => return Err(error).context("failed to read nscd snapshot"),
        }
        thread::sleep(Duration::from_millis(10));
    }
    bail!("nscd snapshot changed while being read")
}

fn parse_nscd_snapshot(contents: &str) -> Option<NscdSnapshot> {
    let mut generation = None;
    let mut generation_end = None;
    let mut pid = None;
    let mut invalidations = Vec::new();
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("generation=") {
            generation = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("generation_end=") {
            generation_end = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("pid=") {
            pid = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("invalidations=") {
            invalidations = value
                .split(',')
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    let generation = generation?;
    if generation == 0 || Some(generation) != generation_end {
        return None;
    }
    Some(NscdSnapshot {
        generation,
        pid: pid?,
        invalidations,
    })
}

fn collect_invalidations() -> Result<Vec<String>> {
    let mut invalidations = Vec::new();
    match fs::read_dir(NSCD_INVALIDATION_DIR) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if let Some(name) = entry.file_name().to_str() {
                    invalidations.push(name.to_string());
                }
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("failed to read nscd invalidations"),
    }
    invalidations.sort();
    invalidations.dedup();
    Ok(invalidations)
}
