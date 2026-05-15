#![no_std]

#[cfg(not(safelibs_dso_build))]
use core::cmp;

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
const DNS_LABEL_POINTER: u8 = 0xc0;
#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
const DNS_LABEL_MASK: u8 = 0xc0;
#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
const DNS_LABEL_MAX: usize = 63;
#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
const DNS_POINTER_SIZE: usize = 2;
#[cfg(not(safelibs_dso_build))]
const DNS_HEADER_LEN: usize = 12;
#[cfg(not(safelibs_dso_build))]
const DNS_RR_FIXED_LEN: usize = 10;
#[cfg(not(safelibs_dso_build))]
const DNS_Q_FIXED_LEN: usize = 4;
#[cfg(not(safelibs_dso_build))]
const DNS_TYPE_PTR: u16 = 12;
#[cfg(not(safelibs_dso_build))]
const DNS_CLASS_IN: u16 = 1;
#[cfg(not(safelibs_dso_build))]
const DNS_MAX_POINTER_DEPTH: usize = 16;
#[cfg(not(safelibs_dso_build))]
const DNS_MAX_LABELS: usize = 128;

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libanl"))]
#[no_mangle]
pub extern "C" fn __libanl_version_placeholder() -> i32 {
    0
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
#[no_mangle]
pub unsafe extern "C" fn ns_get16(src: *const u8) -> u32 {
    read_be16(src)
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
#[no_mangle]
pub unsafe extern "C" fn __ns_get16(src: *const u8) -> u32 {
    read_be16(src)
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
#[no_mangle]
pub unsafe extern "C" fn ns_get32(src: *const u8) -> usize {
    read_be32(src) as usize
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
#[no_mangle]
pub unsafe extern "C" fn __ns_get32(src: *const u8) -> usize {
    read_be32(src) as usize
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
#[no_mangle]
pub unsafe extern "C" fn ns_put16(src: u32, dst: *mut u8) {
    if dst.is_null() {
        return;
    }
    *dst.add(0) = ((src >> 8) & 0xff) as u8;
    *dst.add(1) = (src & 0xff) as u8;
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
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

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
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

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
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

#[cfg(not(safelibs_dso_build))]
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

#[cfg(not(safelibs_dso_build))]
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

#[cfg(not(safelibs_dso_build))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericHostKind {
    Ipv4,
    Ipv6Candidate,
    Name,
    Invalid,
}

#[cfg(not(safelibs_dso_build))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NscdSnapshotHeader {
    pub generation_begin: u64,
    pub payload_len: u32,
    pub generation_end: u64,
}

#[cfg(not(safelibs_dso_build))]
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

#[cfg(not(safelibs_dso_build))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReversePtrResponseStatus {
    ValidPtrAnswer,
    NoPtrAnswer,
    InvalidPtrName,
    MalformedMessage,
}

#[cfg(not(safelibs_dso_build))]
pub fn validate_reverse_ptr_response(message: &[u8]) -> ReversePtrResponseStatus {
    let Some(mut cursor) = skip_dns_header_and_questions(message) else {
        return ReversePtrResponseStatus::MalformedMessage;
    };
    let Some(answer_count) = read_u16_at(message, 6) else {
        return ReversePtrResponseStatus::MalformedMessage;
    };
    let Some(authority_count) = read_u16_at(message, 8) else {
        return ReversePtrResponseStatus::MalformedMessage;
    };
    let Some(additional_count) = read_u16_at(message, 10) else {
        return ReversePtrResponseStatus::MalformedMessage;
    };

    for _ in 0..answer_count {
        let Some((next, rr_type, rr_class, rdata_start, rdata_end)) =
            parse_resource_record(message, cursor)
        else {
            return ReversePtrResponseStatus::MalformedMessage;
        };
        if rr_type == DNS_TYPE_PTR && rr_class == DNS_CLASS_IN {
            return match validate_hostname_name_field(message, rdata_start, rdata_end, 0) {
                Some(end) if end == rdata_end => ReversePtrResponseStatus::ValidPtrAnswer,
                Some(_) => ReversePtrResponseStatus::MalformedMessage,
                None => ReversePtrResponseStatus::InvalidPtrName,
            };
        }
        cursor = next;
    }

    for _ in 0..authority_count.saturating_add(additional_count) {
        let Some((next, _, _, _, _)) = parse_resource_record(message, cursor) else {
            return ReversePtrResponseStatus::MalformedMessage;
        };
        cursor = next;
    }
    if cursor == message.len() {
        ReversePtrResponseStatus::NoPtrAnswer
    } else {
        ReversePtrResponseStatus::MalformedMessage
    }
}

#[cfg(not(safelibs_dso_build))]
pub fn dns_response_is_well_formed(message: &[u8]) -> bool {
    validate_reverse_ptr_response(message) != ReversePtrResponseStatus::MalformedMessage
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
unsafe fn read_be16(src: *const u8) -> u32 {
    if src.is_null() {
        return 0;
    }
    ((*src.add(0) as u32) << 8) | (*src.add(1) as u32)
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
unsafe fn read_be32(src: *const u8) -> u32 {
    if src.is_null() {
        return 0;
    }
    ((*src.add(0) as u32) << 24)
        | ((*src.add(1) as u32) << 16)
        | ((*src.add(2) as u32) << 8)
        | (*src.add(3) as u32)
}

#[cfg(any(not(safelibs_dso_build), safelibs_dso = "libresolv"))]
fn bytes_until(cursor: *const u8, eom: *const u8) -> Option<usize> {
    if cursor > eom {
        None
    } else {
        Some((eom as usize).wrapping_sub(cursor as usize))
    }
}

#[cfg(not(safelibs_dso_build))]
fn skip_dns_header_and_questions(message: &[u8]) -> Option<usize> {
    if message.len() < DNS_HEADER_LEN {
        return None;
    }
    let question_count = read_u16_at(message, 4)?;
    let mut cursor = DNS_HEADER_LEN;
    for _ in 0..question_count {
        cursor = skip_dns_name_field(message, cursor)?;
        cursor = cursor.checked_add(DNS_Q_FIXED_LEN)?;
        if cursor > message.len() {
            return None;
        }
    }
    Some(cursor)
}

#[cfg(not(safelibs_dso_build))]
fn parse_resource_record(message: &[u8], offset: usize) -> Option<(usize, u16, u16, usize, usize)> {
    let metadata = skip_dns_name_field(message, offset)?;
    let rr_type = read_u16_at(message, metadata)?;
    let rr_class = read_u16_at(message, metadata.checked_add(2)?)?;
    let rdlen = read_u16_at(message, metadata.checked_add(8)?)? as usize;
    let rdata_start = metadata.checked_add(DNS_RR_FIXED_LEN)?;
    let rdata_end = rdata_start.checked_add(rdlen)?;
    if rdata_end > message.len() {
        return None;
    }
    Some((rdata_end, rr_type, rr_class, rdata_start, rdata_end))
}

#[cfg(not(safelibs_dso_build))]
fn skip_dns_name_field(message: &[u8], offset: usize) -> Option<usize> {
    let mut cursor = offset;
    let mut labels = 0usize;
    loop {
        let len = *message.get(cursor)?;
        cursor = cursor.checked_add(1)?;
        match len & DNS_LABEL_MASK {
            0 => {
                let label_len = len as usize;
                if label_len > DNS_LABEL_MAX {
                    return None;
                }
                if label_len == 0 {
                    return Some(cursor);
                }
                labels = labels.checked_add(1)?;
                if labels > DNS_MAX_LABELS {
                    return None;
                }
                cursor = cursor.checked_add(label_len)?;
                if cursor > message.len() {
                    return None;
                }
            }
            DNS_LABEL_POINTER => {
                let next = *message.get(cursor)?;
                let target = (((len & !DNS_LABEL_MASK) as usize) << 8) | next as usize;
                if target >= message.len() {
                    return None;
                }
                validate_dns_name_target(message, target, 0)?;
                return cursor.checked_add(1);
            }
            _ => return None,
        }
    }
}

#[cfg(not(safelibs_dso_build))]
fn validate_dns_name_target(message: &[u8], offset: usize, depth: usize) -> Option<()> {
    if depth > DNS_MAX_POINTER_DEPTH {
        return None;
    }
    let mut cursor = offset;
    let mut labels = 0usize;
    loop {
        let len = *message.get(cursor)?;
        cursor = cursor.checked_add(1)?;
        match len & DNS_LABEL_MASK {
            0 => {
                let label_len = len as usize;
                if label_len > DNS_LABEL_MAX {
                    return None;
                }
                if label_len == 0 {
                    return Some(());
                }
                labels = labels.checked_add(1)?;
                if labels > DNS_MAX_LABELS {
                    return None;
                }
                cursor = cursor.checked_add(label_len)?;
                if cursor > message.len() {
                    return None;
                }
            }
            DNS_LABEL_POINTER => {
                let next = *message.get(cursor)?;
                let target = (((len & !DNS_LABEL_MASK) as usize) << 8) | next as usize;
                if target >= message.len() {
                    return None;
                }
                return validate_dns_name_target(message, target, depth.checked_add(1)?);
            }
            _ => return None,
        }
    }
}

#[cfg(not(safelibs_dso_build))]
fn validate_hostname_name_field(
    message: &[u8],
    offset: usize,
    field_end: usize,
    depth: usize,
) -> Option<usize> {
    if depth > DNS_MAX_POINTER_DEPTH || offset >= field_end || field_end > message.len() {
        return None;
    }
    let mut cursor = offset;
    let mut labels = 0usize;
    loop {
        if cursor >= field_end {
            return None;
        }
        let len = *message.get(cursor)?;
        cursor = cursor.checked_add(1)?;
        match len & DNS_LABEL_MASK {
            0 => {
                let label_len = len as usize;
                if label_len > DNS_LABEL_MAX {
                    return None;
                }
                if label_len == 0 {
                    return Some(cursor);
                }
                labels = labels.checked_add(1)?;
                if labels > DNS_MAX_LABELS {
                    return None;
                }
                let end = cursor.checked_add(label_len)?;
                if end > field_end || !hostname_label_is_valid(&message[cursor..end]) {
                    return None;
                }
                cursor = end;
            }
            DNS_LABEL_POINTER => {
                if cursor >= field_end {
                    return None;
                }
                let next = *message.get(cursor)?;
                let target = (((len & !DNS_LABEL_MASK) as usize) << 8) | next as usize;
                if target >= message.len() {
                    return None;
                }
                validate_hostname_target(message, target, depth.checked_add(1)?)?;
                return cursor.checked_add(1);
            }
            _ => return None,
        }
    }
}

#[cfg(not(safelibs_dso_build))]
fn validate_hostname_target(message: &[u8], offset: usize, depth: usize) -> Option<()> {
    if depth > DNS_MAX_POINTER_DEPTH {
        return None;
    }
    let mut cursor = offset;
    let mut labels = 0usize;
    loop {
        let len = *message.get(cursor)?;
        cursor = cursor.checked_add(1)?;
        match len & DNS_LABEL_MASK {
            0 => {
                let label_len = len as usize;
                if label_len > DNS_LABEL_MAX {
                    return None;
                }
                if label_len == 0 {
                    return Some(());
                }
                labels = labels.checked_add(1)?;
                if labels > DNS_MAX_LABELS {
                    return None;
                }
                let end = cursor.checked_add(label_len)?;
                if end > message.len() || !hostname_label_is_valid(&message[cursor..end]) {
                    return None;
                }
                cursor = end;
            }
            DNS_LABEL_POINTER => {
                let next = *message.get(cursor)?;
                let target = (((len & !DNS_LABEL_MASK) as usize) << 8) | next as usize;
                if target >= message.len() {
                    return None;
                }
                return validate_hostname_target(message, target, depth.checked_add(1)?);
            }
            _ => return None,
        }
    }
}

#[cfg(not(safelibs_dso_build))]
fn hostname_label_is_valid(label: &[u8]) -> bool {
    !label.is_empty()
        && label.len() <= DNS_LABEL_MAX
        && label[0] != b'-'
        && label[label.len() - 1] != b'-'
        && label
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

#[cfg(not(safelibs_dso_build))]
fn read_u16_at(message: &[u8], offset: usize) -> Option<u16> {
    let hi = *message.get(offset)?;
    let lo = *message.get(offset.checked_add(1)?)?;
    Some(u16::from_be_bytes([hi, lo]))
}

#[cfg(not(safelibs_dso_build))]
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

#[cfg(not(safelibs_dso_build))]
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

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn ptr_question() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&[
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        packet.extend_from_slice(&[
            1, b'4', 1, b'3', 1, b'2', 1, b'1', 7, b'i', b'n', b'-', b'a', b'd', b'd', b'r', 4,
            b'a', b'r', b'p', b'a', 0,
        ]);
        packet.extend_from_slice(&[0x00, DNS_TYPE_PTR as u8, 0x00, DNS_CLASS_IN as u8]);
        packet
    }

    fn append_counts(packet: &mut [u8], answers: u16, authorities: u16, additionals: u16) {
        packet[6..8].copy_from_slice(&answers.to_be_bytes());
        packet[8..10].copy_from_slice(&authorities.to_be_bytes());
        packet[10..12].copy_from_slice(&additionals.to_be_bytes());
    }

    fn append_ptr_rr(packet: &mut Vec<u8>, owner_offset: u8, target_labels: &[&[u8]]) {
        packet.extend_from_slice(&[
            DNS_LABEL_POINTER,
            owner_offset,
            0x00,
            DNS_TYPE_PTR as u8,
            0x00,
            DNS_CLASS_IN as u8,
            0x00,
            0x00,
            0x00,
            0x3c,
        ]);
        let rdlen_at = packet.len();
        packet.extend_from_slice(&[0x00, 0x00]);
        let start = packet.len();
        for label in target_labels {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label);
        }
        packet.push(0);
        let rdlen = (packet.len() - start) as u16;
        packet[rdlen_at..rdlen_at + 2].copy_from_slice(&rdlen.to_be_bytes());
    }

    #[test]
    fn reverse_ptr_accepts_valid_answer_section_ptr() {
        let mut packet = ptr_question();
        append_counts(&mut packet, 1, 0, 0);
        append_ptr_rr(&mut packet, 12, &[b"localhost"]);

        assert_eq!(
            validate_reverse_ptr_response(&packet),
            ReversePtrResponseStatus::ValidPtrAnswer
        );
        assert!(dns_response_is_well_formed(&packet));
    }

    #[test]
    fn reverse_ptr_ignores_authority_section_ptr() {
        let mut packet = ptr_question();
        append_counts(&mut packet, 0, 1, 0);
        append_ptr_rr(&mut packet, 12, &[b"not-answer", b"example"]);

        assert_eq!(
            validate_reverse_ptr_response(&packet),
            ReversePtrResponseStatus::NoPtrAnswer
        );
    }

    #[test]
    fn reverse_ptr_rejects_invalid_answer_hostname() {
        let mut packet = ptr_question();
        append_counts(&mut packet, 1, 0, 0);
        append_ptr_rr(&mut packet, 12, &[b"-bad", b"example"]);

        assert_eq!(
            validate_reverse_ptr_response(&packet),
            ReversePtrResponseStatus::InvalidPtrName
        );
    }

    #[test]
    fn reverse_ptr_rejects_malformed_messages() {
        assert_eq!(
            validate_reverse_ptr_response(&[0; DNS_HEADER_LEN - 1]),
            ReversePtrResponseStatus::MalformedMessage
        );

        let mut packet = ptr_question();
        append_counts(&mut packet, 1, 0, 0);
        packet.extend_from_slice(&[
            DNS_LABEL_POINTER,
            0xff,
            0x00,
            DNS_TYPE_PTR as u8,
            0x00,
            DNS_CLASS_IN as u8,
            0x00,
            0x00,
            0x00,
            0x3c,
            0x00,
            0x02,
            DNS_LABEL_POINTER,
            0xff,
        ]);
        assert_eq!(
            validate_reverse_ptr_response(&packet),
            ReversePtrResponseStatus::MalformedMessage
        );
    }

    #[test]
    fn numeric_host_kind_rejects_edge_cases() {
        assert_eq!(numeric_host_kind(b"127.0.0.1"), NumericHostKind::Ipv4);
        assert_eq!(numeric_host_kind(b"127.000.0.1"), NumericHostKind::Name);
        assert_eq!(numeric_host_kind(b"127.0.0.256"), NumericHostKind::Name);
        assert_eq!(
            numeric_host_kind(b"2001:db8::1"),
            NumericHostKind::Ipv6Candidate
        );
        assert_eq!(numeric_host_kind(b"bad host"), NumericHostKind::Invalid);
    }

    #[test]
    fn nscd_snapshot_rejects_torn_payloads() {
        let header = NscdSnapshotHeader {
            generation_begin: 4,
            payload_len: 12,
            generation_end: 5,
        };
        assert_eq!(header.checked_payload_len(12), None);

        let header = NscdSnapshotHeader {
            generation_begin: 4,
            payload_len: 8,
            generation_end: 4,
        };
        assert_eq!(header.checked_payload_len(12), Some(8));
    }
}
