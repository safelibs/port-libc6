use anyhow::{anyhow, Context, Result};
use std::fs;
use std::mem::size_of;

pub const AT_NULL: usize = 0;
pub const AT_IGNORE: usize = 1;
pub const AT_EXECFD: usize = 2;
pub const AT_PHDR: usize = 3;
pub const AT_PHENT: usize = 4;
pub const AT_PHNUM: usize = 5;
pub const AT_PAGESZ: usize = 6;
pub const AT_BASE: usize = 7;
pub const AT_FLAGS: usize = 8;
pub const AT_ENTRY: usize = 9;
pub const AT_NOTELF: usize = 10;
pub const AT_UID: usize = 11;
pub const AT_EUID: usize = 12;
pub const AT_GID: usize = 13;
pub const AT_EGID: usize = 14;
pub const AT_PLATFORM: usize = 15;
pub const AT_HWCAP: usize = 16;
pub const AT_CLKTCK: usize = 17;
pub const AT_FPUCW: usize = 18;
pub const AT_SECURE: usize = 23;
pub const AT_BASE_PLATFORM: usize = 24;
pub const AT_RANDOM: usize = 25;
pub const AT_HWCAP2: usize = 26;
pub const AT_EXECFN: usize = 31;
pub const AT_SYSINFO: usize = 32;
pub const AT_SYSINFO_EHDR: usize = 33;
pub const AT_L1I_CACHESHAPE: usize = 34;
pub const AT_L1D_CACHESHAPE: usize = 35;
pub const AT_L2_CACHESHAPE: usize = 36;
pub const AT_L3_CACHESHAPE: usize = 37;
pub const AT_HWCAP3: usize = 48;
pub const AT_HWCAP4: usize = 49;
pub const AT_MINSIGSTKSZ: usize = 51;

const EXEC_PAGESIZE_DEFAULT: usize = 4096;
const DEFAULT_FPUCW: usize = 0x037f;
const CONSTANT_MINSIGSTKSZ: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuxEntry {
    pub key: usize,
    pub value: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuxValues {
    values: Vec<usize>,
}

impl AuxValues {
    pub fn new() -> Self {
        let mut values = vec![0; AT_MINSIGSTKSZ + 1];
        values[AT_PAGESZ] = EXEC_PAGESIZE_DEFAULT;
        values[AT_FPUCW] = DEFAULT_FPUCW;
        values[AT_MINSIGSTKSZ] = CONSTANT_MINSIGSTKSZ;
        Self { values }
    }

    pub fn get(&self, key: usize) -> usize {
        self.values.get(key).copied().unwrap_or(0)
    }

    pub fn pagesize(&self) -> usize {
        self.get(AT_PAGESZ)
    }

    pub fn secure(&self) -> bool {
        self.get(AT_SECURE) != 0
    }

    pub fn platform_ptr(&self) -> usize {
        self.get(AT_PLATFORM)
    }

    pub fn hwcap(&self) -> usize {
        self.get(AT_HWCAP)
    }

    pub fn hwcap2(&self) -> usize {
        self.get(AT_HWCAP2)
    }

    pub fn hwcap3(&self) -> usize {
        self.get(AT_HWCAP3)
    }

    pub fn hwcap4(&self) -> usize {
        self.get(AT_HWCAP4)
    }

    pub fn clktck(&self) -> usize {
        self.get(AT_CLKTCK)
    }

    pub fn fpu_control(&self) -> usize {
        self.get(AT_FPUCW)
    }

    pub fn random_ptr(&self) -> usize {
        self.get(AT_RANDOM)
    }

    pub fn minsigstacksize(&self) -> usize {
        self.get(AT_MINSIGSTKSZ)
    }

    pub fn sysinfo(&self) -> usize {
        self.get(AT_SYSINFO)
    }

    pub fn sysinfo_ehdr(&self) -> usize {
        self.get(AT_SYSINFO_EHDR)
    }
}

impl Default for AuxValues {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_auxv(entries: &[AuxEntry]) -> AuxValues {
    let mut values = AuxValues::new();
    for entry in entries {
        if entry.key == AT_NULL {
            break;
        }
        if entry.key <= AT_MINSIGSTKSZ {
            values.values[entry.key] = entry.value;
        }
    }
    values
}

pub fn current_process_auxv() -> Result<AuxValues> {
    let bytes = fs::read("/proc/self/auxv").context("failed to read /proc/self/auxv")?;
    let word = size_of::<usize>();
    if bytes.len() % (word * 2) != 0 {
        return Err(anyhow!("unexpected auxv size {}", bytes.len()));
    }

    let mut entries = Vec::with_capacity(bytes.len() / (word * 2));
    for chunk in bytes.chunks_exact(word * 2) {
        let key = usize::from_ne_bytes(
            chunk[..word]
                .try_into()
                .map_err(|_| anyhow!("failed to decode auxv key"))?,
        );
        let value = usize::from_ne_bytes(
            chunk[word..]
                .try_into()
                .map_err(|_| anyhow!("failed to decode auxv value"))?,
        );
        entries.push(AuxEntry { key, value });
        if key == AT_NULL {
            break;
        }
    }
    Ok(parse_auxv(&entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_linux_defaults_before_copying_entries() {
        let values = parse_auxv(&[]);
        assert_eq!(values.pagesize(), EXEC_PAGESIZE_DEFAULT);
        assert_eq!(values.fpu_control(), DEFAULT_FPUCW);
        assert_eq!(values.minsigstacksize(), CONSTANT_MINSIGSTKSZ);
    }

    #[test]
    fn copies_known_entries_and_ignores_unknown_tags() {
        let values = parse_auxv(&[
            AuxEntry {
                key: AT_PAGESZ,
                value: 8192,
            },
            AuxEntry {
                key: AT_SECURE,
                value: 1,
            },
            AuxEntry {
                key: AT_HWCAP2,
                value: 0x55,
            },
            AuxEntry {
                key: AT_MINSIGSTKSZ + 10,
                value: 1234,
            },
            AuxEntry {
                key: AT_NULL,
                value: 0,
            },
        ]);
        assert_eq!(values.pagesize(), 8192);
        assert!(values.secure());
        assert_eq!(values.hwcap2(), 0x55);
        assert_eq!(values.get(AT_MINSIGSTKSZ + 10), 0);
    }
}
