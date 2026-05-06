#![no_std]

use core::cmp;

const DNS_LABEL_POINTER: u8 = 0xc0;
const DNS_LABEL_MASK: u8 = 0xc0;
const DNS_LABEL_MAX: usize = 63;
const DNS_POINTER_SIZE: usize = 2;

#[no_mangle]
pub extern "C" fn __libanl_version_placeholder() -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn ns_get16(src: *const u8) -> u32 {
    read_be16(src)
}

#[no_mangle]
pub unsafe extern "C" fn __ns_get16(src: *const u8) -> u32 {
    read_be16(src)
}

#[no_mangle]
pub unsafe extern "C" fn ns_get32(src: *const u8) -> usize {
    read_be32(src) as usize
}

#[no_mangle]
pub unsafe extern "C" fn __ns_get32(src: *const u8) -> usize {
    read_be32(src) as usize
}

#[no_mangle]
pub unsafe extern "C" fn ns_put16(src: u32, dst: *mut u8) {
    if dst.is_null() {
        return;
    }
    *dst.add(0) = ((src >> 8) & 0xff) as u8;
    *dst.add(1) = (src & 0xff) as u8;
}

#[no_mangle]
pub unsafe extern "C" fn ns_put32(src: usize, dst: *mut u8) {
    if dst.is_null() {
        return;
    }
    let value = src as u32;
    *dst.add(0) = ((value >> 24) & 0xff) as u8;
    *dst.add(1) = ((value >> 16) & 0xff) as u8;
    *dst.add(2) = ((value >> 8) & 0xff) as u8;
    *dst.add(3) = (value & 0xff) as u8;
}

#[no_mangle]
pub unsafe extern "C" fn ns_name_skip(ptrptr: *mut *const u8, eom: *const u8) -> i32 {
    if ptrptr.is_null() || eom.is_null() {
        return -1;
    }
    let start = *ptrptr;
    if start.is_null() || start > eom {
        return -1;
    }
    match skip_dns_name(start, eom) {
        Some(next) => {
            *ptrptr = next;
            0
        }
        None => -1,
    }
}

pub fn skip_dns_name(mut cursor: *const u8, eom: *const u8) -> Option<*const u8> {
    loop {
        if cursor >= eom {
            return None;
        }
        let len = unsafe { *cursor };
        cursor = unsafe { cursor.add(1) };
        match len & DNS_LABEL_MASK {
            0 => {
                let label_len = len as usize;
                if label_len > DNS_LABEL_MAX {
                    return None;
                }
                if label_len == 0 {
                    return Some(cursor);
                }
                if bytes_until(cursor, eom)? < label_len {
                    return None;
                }
                cursor = unsafe { cursor.add(label_len) };
            }
            DNS_LABEL_POINTER => {
                if bytes_until(cursor, eom)? < DNS_POINTER_SIZE - 1 {
                    return None;
                }
                return Some(unsafe { cursor.add(1) });
            }
            _ => return None,
        }
    }
}

pub fn reverse_lookup_name_is_valid(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    let mut labels = name.split(|byte| *byte == b'.').peekable();
    while let Some(label) = labels.next() {
        if label.is_empty() {
            return labels.peek().is_none();
        }
        if label.len() > DNS_LABEL_MAX {
            return false;
        }
        if label[0] == b'-' || label[label.len() - 1] == b'-' {
            return false;
        }
        if !label
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return false;
        }
    }
    true
}

pub fn numeric_host_kind(input: &[u8]) -> NumericHostKind {
    if input.is_empty() || input.iter().any(|byte| byte.is_ascii_whitespace()) {
        return NumericHostKind::Invalid;
    }
    let dotted_quad = input
        .split(|byte| *byte == b'.')
        .collect::<heapless_parts::Parts<4>>();
    if dotted_quad.len() == 4
        && dotted_quad
            .iter()
            .all(|part| parse_decimal_octet(part).is_some())
    {
        return NumericHostKind::Ipv4;
    }
    if input
        .iter()
        .all(|byte| byte.is_ascii_hexdigit() || *byte == b':' || *byte == b'.')
        && input.contains(&b':')
    {
        return NumericHostKind::Ipv6Candidate;
    }
    if input
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'-' || *byte == b'_')
    {
        NumericHostKind::Name
    } else {
        NumericHostKind::Invalid
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericHostKind {
    Ipv4,
    Ipv6Candidate,
    Name,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NscdSnapshotHeader {
    pub generation_begin: u64,
    pub payload_len: u32,
    pub generation_end: u64,
}

impl NscdSnapshotHeader {
    pub fn checked_payload_len(&self, actual_len: usize) -> Option<usize> {
        let payload_len = self.payload_len as usize;
        if self.generation_begin == 0
            || self.generation_begin != self.generation_end
            || payload_len > actual_len
        {
            return None;
        }
        Some(cmp::min(payload_len, actual_len))
    }
}

unsafe fn read_be16(src: *const u8) -> u32 {
    if src.is_null() {
        return 0;
    }
    ((*src.add(0) as u32) << 8) | (*src.add(1) as u32)
}

unsafe fn read_be32(src: *const u8) -> u32 {
    if src.is_null() {
        return 0;
    }
    ((*src.add(0) as u32) << 24)
        | ((*src.add(1) as u32) << 16)
        | ((*src.add(2) as u32) << 8)
        | (*src.add(3) as u32)
}

fn bytes_until(cursor: *const u8, eom: *const u8) -> Option<usize> {
    if cursor > eom {
        None
    } else {
        Some((eom as usize).wrapping_sub(cursor as usize))
    }
}

fn parse_decimal_octet(input: &[u8]) -> Option<u8> {
    if input.is_empty() || input.len() > 3 || (input.len() > 1 && input[0] == b'0') {
        return None;
    }
    let mut value = 0u16;
    for byte in input {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (*byte - b'0') as u16;
        if value > 255 {
            return None;
        }
    }
    Some(value as u8)
}

mod heapless_parts {
    pub struct Parts<'a, const N: usize> {
        inner: [&'a [u8]; N],
        len: usize,
        overflow: bool,
    }

    impl<'a, const N: usize> Parts<'a, N> {
        pub fn len(&self) -> usize {
            if self.overflow {
                N + 1
            } else {
                self.len
            }
        }

        pub fn iter(&self) -> impl Iterator<Item = &&'a [u8]> {
            self.inner[..self.len].iter()
        }
    }

    impl<'a, const N: usize> core::iter::FromIterator<&'a [u8]> for Parts<'a, N> {
        fn from_iter<T: IntoIterator<Item = &'a [u8]>>(iter: T) -> Self {
            let mut inner = [&[][..]; N];
            let mut len = 0usize;
            let mut overflow = false;
            for item in iter {
                if len == N {
                    overflow = true;
                    break;
                }
                inner[len] = item;
                len += 1;
            }
            Self {
                inner,
                len,
                overflow,
            }
        }
    }
}
