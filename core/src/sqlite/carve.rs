//! Raw SQLite page carving: salvage deleted rows without going through SQLite.
//!
//! Strategies:
//! 1. **Freeblocks** inside live table-leaf pages — when a row is deleted its
//!    space becomes a freeblock; the record bytes usually survive until reused.
//! 2. **Freelist pages** — pages fully released back to the database (tracked
//!    via the freelist trunk chain in the db header).
//! 3. **WAL frames** — page versions that only exist in the `-wal` file.
//!
//! Salvaged records are filtered with a message-shape heuristic (a non-empty
//! TEXT column plus a plausible millisecond timestamp).

use super::{Message, Source};
use crate::error::Result;
use std::collections::HashSet;
use std::path::Path;

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
/// Plausible WhatsApp timestamp window (ms): 2009-01-01 .. 2100-01-01.
const TS_MIN: i64 = 1_230_768_000_000;
const TS_MAX: i64 = 4_102_444_800_000;

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// Read a SQLite varint (big-endian, 7 bits/byte, 9th byte uses all 8 bits).
pub fn read_varint(buf: &[u8], off: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    for i in 0..9 {
        let b = *buf.get(off + i)?;
        if i == 8 {
            result = (result << 8) | u64::from(b);
            return Some((result, off + 9));
        }
        result = (result << 7) | u64::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Some((result, off + i + 1));
        }
    }
    None
}

/// Size in bytes of an integer serial type. Note SQLite skips 5- and 7-byte
/// integers: type 5 stores a 48-bit (6-byte) value, type 6 a 64-bit value.
fn int_size(t: u64) -> usize {
    match t {
        5 => 6,
        6 => 8,
        _ => t as usize,
    }
}

/// Decode record values for a known serial-type list. Returns the values and
/// how many data bytes were consumed.
fn decode_values(buf: &[u8], types: &[u64]) -> Option<(Vec<CellValue>, usize)> {
    let mut values = Vec::new();
    let mut data = 0usize;
    for &t in types {
        match t {
            0 => values.push(CellValue::Null),
            1..=6 => {
                let n = int_size(t);
                let end = data.checked_add(n)?;
                if end > buf.len() {
                    return None;
                }
                let mut v: i64 = if buf[data] & 0x80 != 0 { -1 } else { 0 };
                for &b in &buf[data..end] {
                    v = (v << 8) | i64::from(b);
                }
                values.push(CellValue::Int(v));
                data = end;
            }
            7 => {
                if data + 8 > buf.len() {
                    return None;
                }
                let mut a = [0u8; 8];
                a.copy_from_slice(&buf[data..data + 8]);
                values.push(CellValue::Real(f64::from_be_bytes(a)));
                data += 8;
            }
            8 => values.push(CellValue::Int(0)),
            9 => values.push(CellValue::Int(1)),
            _ if t >= 12 => {
                let len = if t % 2 == 0 {
                    (t - 12) / 2
                } else {
                    (t - 13) / 2
                } as usize;
                let end = data.checked_add(len)?;
                if end > buf.len() {
                    return None;
                }
                if t % 2 == 0 {
                    values.push(CellValue::Blob(buf[data..end].to_vec()));
                } else {
                    let s = String::from_utf8_lossy(&buf[data..end]).into_owned();
                    // Reject records containing obvious binary garbage in text.
                    if s.contains('\0') {
                        return None;
                    }
                    values.push(CellValue::Text(s));
                }
                data = end;
            }
            _ => return None,
        }
    }
    Some((values, data))
}

/// Decode a SQLite record (header with serial types followed by values).
pub fn parse_record(buf: &[u8]) -> Option<Vec<CellValue>> {
    let (header_len_v, mut off) = read_varint(buf, 0)?;
    let header_len = header_len_v as usize;
    if header_len == 0 || header_len > buf.len() {
        return None;
    }
    let mut types = Vec::new();
    while off < header_len {
        let (t, o) = read_varint(buf, off)?;
        types.push(t);
        off = o;
    }
    let (values, _) = decode_values(&buf[header_len..], &types)?;
    Some(values)
}

/// Valid single-byte serial types (10/11 are reserved and never appear).
fn valid_serial_type(b: u8) -> bool {
    b <= 9 || b >= 12
}

/// Salvage a record whose first 1-2 bytes were destroyed.
///
/// When SQLite frees a cell, the 4-byte freeblock header (next + size)
/// overwrites the start of the cell: the payload-length varint, the rowid
/// varint, and possibly the first bytes of the record itself. This function
/// tries to rebuild such a damaged record by guessing the missing prefix and
/// requiring that the decoded values consume the buffer exactly.
fn reconstruct_damaged_record(content: &[u8]) -> Option<Vec<CellValue>> {
    // `loss` = how many record bytes (header-length byte + possibly the first
    // serial type) were destroyed, after the cell varints were accounted for.
    for loss in 1..=2 {
        // `s` = number of surviving serial-type bytes at the start of content.
        for s in 1..content.len().min(48) {
            let type_bytes = &content[..s];
            if !type_bytes.iter().all(|&b| valid_serial_type(b)) {
                continue;
            }
            let rest = &content[s..];
            // loss == 1: header-length byte gone, all serial types survive.
            if loss == 1 {
                let types: Vec<u64> = type_bytes.iter().map(|&b| b as u64).collect();
                if let Some((values, consumed)) = decode_values(rest, &types) {
                    if consumed == rest.len() && !values.is_empty() {
                        return Some(values);
                    }
                }
            }
            // loss == 2: header-length byte AND the first serial type gone;
            // guess the first type (small integer/null types are by far the
            // most common leading columns).
            if loss == 2 {
                for t0 in 0u64..=9 {
                    let mut types = Vec::with_capacity(s + 1);
                    types.push(t0);
                    types.extend(type_bytes.iter().map(|&b| b as u64));
                    if let Some((values, consumed)) = decode_values(rest, &types) {
                        if consumed == rest.len() && !values.is_empty() {
                            return Some(values);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Heuristic: does this record look like a WhatsApp message row?
/// Requires a non-empty TEXT column and a plausible ms timestamp. The longest
/// TEXT column is taken as the message body (ids/jids are short).
pub fn message_from_values(values: &[CellValue], source: Source) -> Option<Message> {
    let text = values
        .iter()
        .filter_map(|v| match v {
            CellValue::Text(t) if !t.trim().is_empty() && t.len() <= 65_536 => Some(t.clone()),
            _ => None,
        })
        .max_by_key(|t| t.len())?;
    let timestamp = values.iter().find_map(|v| match v {
        CellValue::Int(i) if (TS_MIN..TS_MAX).contains(i) => Some(*i),
        _ => None,
    })?;
    Some(Message {
        row_id: None,
        chat_jid: None,
        from_me: false,
        sender: None,
        text: Some(text),
        timestamp: Some(timestamp),
        source,
    })
}

type Seen = HashSet<(i64, String)>;

fn dedup_key(m: &Message) -> (i64, String) {
    (m.timestamp.unwrap_or(0), m.text.clone().unwrap_or_default())
}

fn try_record(bytes: &[u8], source: Source, out: &mut Vec<Message>, seen: &mut Seen) {
    // A salvaged blob may be a bare record, or it may carry a leftover cell
    // prefix (varint payload length + varint rowid). Try both.
    for candidate in record_candidates(bytes) {
        if let Some(values) = parse_record(&candidate) {
            if let Some(m) = message_from_values(&values, source) {
                if seen.insert(dedup_key(&m)) {
                    out.push(m);
                }
                return;
            }
        }
    }
    // Freeblocks destroy the first bytes of the freed cell; try rebuilding.
    if let Some(values) = reconstruct_damaged_record(bytes) {
        if let Some(m) = message_from_values(&values, source) {
            if seen.insert(dedup_key(&m)) {
                out.push(m);
            }
        }
    }
}

fn record_candidates(bytes: &[u8]) -> Vec<&[u8]> {
    let mut cands = vec![bytes];
    // Skip a possible cell header: payload_len varint + rowid varint.
    if let Some((_, o1)) = read_varint(bytes, 0) {
        if let Some((_, o2)) = read_varint(bytes, o1) {
            if o2 < bytes.len() {
                cands.push(&bytes[o2..]);
            }
        }
    }
    cands
}

/// Carve freeblocks (deleted row space) from a table-leaf page.
fn carve_freeblocks(page: &[u8], source: Source, out: &mut Vec<Message>, seen: &mut Seen) {
    if page.len() < 8 || page[0] != 0x0d {
        return;
    }
    let mut fb = u16::from_be_bytes([page[1], page[2]]) as usize;
    let mut guard = 0;
    while fb != 0 && fb + 4 <= page.len() && guard < 512 {
        guard += 1;
        let next = u16::from_be_bytes([page[fb], page[fb + 1]]) as usize;
        let size = u16::from_be_bytes([page[fb + 2], page[fb + 3]]) as usize;
        if size >= 6 && fb + size <= page.len() {
            try_record(&page[fb + 4..fb + size], source, out, seen);
        }
        if next <= fb {
            break; // protect against corrupted chains
        }
        fb = next;
    }
}

/// Carve live cell payloads from a table-leaf page (used for freelist pages
/// and WAL page versions, where the cells themselves hold lost rows).
fn carve_leaf_cells(page: &[u8], usable: usize, source: Source, out: &mut Vec<Message>, seen: &mut Seen) {
    if page.len() < 8 || page[0] != 0x0d {
        return;
    }
    let ncells = u16::from_be_bytes([page[3], page[4]]) as usize;
    for i in 0..ncells {
        let ptr_off = 8 + i * 2;
        if ptr_off + 2 > page.len() {
            break;
        }
        let cell_off = u16::from_be_bytes([page[ptr_off], page[ptr_off + 1]]) as usize;
        if cell_off >= page.len() {
            continue;
        }
        carve_cell_payload(page, cell_off, usable, source, out, seen);
    }
}

fn carve_cell_payload(
    page: &[u8],
    off: usize,
    usable: usize,
    source: Source,
    out: &mut Vec<Message>,
    seen: &mut Seen,
) {
    let Some((payload_len, o1)) = read_varint(page, off) else {
        return;
    };
    let Some((_, o2)) = read_varint(page, o1) else {
        return;
    };
    let payload_len = payload_len as usize;
    // SQLite local-payload threshold (overflow pages are not followed in MVP).
    let x = usable.saturating_sub(35);
    let local = if payload_len <= x {
        payload_len
    } else {
        let m = (usable.saturating_sub(12) * 32) / 255 - 23;
        let k = m + (payload_len - m) % usable.saturating_sub(4).max(1);
        if k <= x {
            k
        } else {
            m
        }
    };
    let end = page.len().min(o2 + local);
    if o2 < end {
        try_record(&page[o2..end], source, out, seen);
    }
}

/// Carve a whole database file: freeblocks of live pages + freelist pages.
pub fn carve_db(path: &Path, source: Source) -> Result<Vec<Message>> {
    let data = std::fs::read(path)?;
    if data.len() < 100 || &data[0..16] != SQLITE_MAGIC {
        return Ok(Vec::new());
    }
    let raw_ps = u16::from_be_bytes([data[16], data[17]]);
    let page_size = if raw_ps == 1 { 65536 } else { raw_ps as usize };
    if page_size < 512 || !page_size.is_power_of_two() {
        return Ok(Vec::new());
    }
    let reserved = data[20] as usize;
    let usable = page_size.saturating_sub(reserved);
    let n_pages = data.len() / page_size;

    let mut out = Vec::new();
    let mut seen = Seen::new();

    // 1) Freelist pages (fully deleted pages) via the trunk chain at header offset 32.
    let mut trunk = u32::from_be_bytes([data[32], data[33], data[34], data[35]]) as usize;
    let mut guard = 0;
    while trunk != 0 && trunk <= n_pages && guard < 4096 {
        guard += 1;
        let base = (trunk - 1) * page_size;
        if base + 8 > data.len() {
            break;
        }
        let next = u32::from_be_bytes(data[base..base + 4].try_into().unwrap()) as usize;
        let nleaf = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap()) as usize;
        for i in 0..nleaf.min((page_size - 8) / 4) {
            let p = u32::from_be_bytes(data[base + 8 + i * 4..base + 12 + i * 4].try_into().unwrap())
                as usize;
            if (1..=n_pages).contains(&p) {
                carve_leaf_cells(&data[(p - 1) * page_size..p * page_size], usable, source, &mut out, &mut seen);
            }
        }
        if next == trunk {
            break;
        }
        trunk = next;
    }

    // 2) Freeblocks inside all table-leaf pages (rows deleted in place).
    for p in 1..=n_pages {
        carve_freeblocks(&data[(p - 1) * page_size..p * page_size], source, &mut out, &mut seen);
    }

    Ok(out)
}

/// Carve page versions stored in a `-wal` file.
pub fn carve_wal(wal_path: &Path, source: Source) -> Result<Vec<Message>> {
    let data = std::fs::read(wal_path)?;
    if data.len() < 32 {
        return Ok(Vec::new());
    }
    let magic = u32::from_be_bytes(data[0..4].try_into().unwrap());
    if magic != 0x377f_0682 && magic != 0x377f_0683 {
        return Ok(Vec::new());
    }
    let page_size = u32::from_be_bytes(data[8..12].try_into().unwrap()) as usize;
    if page_size < 512 || !page_size.is_power_of_two() {
        return Ok(Vec::new());
    }
    let salt = &data[16..24];

    // Latest frame per page number wins.
    let mut latest: std::collections::HashMap<u32, &[u8]> = std::collections::HashMap::new();
    let mut off = 32;
    while off + 24 + page_size <= data.len() {
        let frame = &data[off..off + 24 + page_size];
        if &frame[8..16] != salt {
            break; // frames from an older WAL generation
        }
        let pgno = u32::from_be_bytes(frame[0..4].try_into().unwrap());
        latest.insert(pgno, &frame[24..24 + page_size]);
        off += 24 + page_size;
    }

    let mut out = Vec::new();
    let mut seen = Seen::new();
    for page in latest.values() {
        carve_leaf_cells(page, page_size, source, &mut out, &mut seen);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_cases() {
        // single byte
        let (v, n) = read_varint(&[0x07], 0).unwrap();
        assert_eq!((v, n), (7, 1));
        // two bytes: 300 = 0b100101100 -> 0x82 0x2c
        let (v, n) = read_varint(&[0x82, 0x2c], 0).unwrap();
        assert_eq!((v, n), (300, 2));
    }

    #[test]
    fn record_parse_text_and_int() {
        // header: varint length 3 (counts itself + 2 serial-type bytes)
        let rec = [
            0x03, // header length = 3
            0x11, // serial type 17 -> TEXT of length 2 (2*2+13)
            0x04, // serial type 4 -> 4-byte int
            b'h', b'i', // text "hi"
            0x00, 0x00, 0x01, 0x00, // int 256
        ];
        let values = parse_record(&rec).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], CellValue::Text("hi".into()));
        assert_eq!(values[1], CellValue::Int(256));
    }

    #[test]
    fn message_heuristic_requires_text_and_timestamp() {
        let good = vec![
            CellValue::Int(42),
            CellValue::Text("hello world".into()),
            CellValue::Int(1_700_000_000_000),
        ];
        assert!(message_from_values(&good, Source::Freelist).is_some());

        let no_ts = vec![CellValue::Text("hello".into()), CellValue::Int(5)];
        assert!(message_from_values(&no_ts, Source::Freelist).is_none());

        let no_text = vec![CellValue::Int(1_700_000_000_000)];
        assert!(message_from_values(&no_text, Source::Freelist).is_none());
    }
}
