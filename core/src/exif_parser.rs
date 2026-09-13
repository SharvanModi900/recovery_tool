//! Pure-Rust EXIF metadata parser for JPEG/TIFF images.
//!
//! Extracts GPS coordinates, camera info, timestamps, and software tags
//! from image files already pulled to local storage.

use serde::{Deserialize, Serialize};

/// Parsed EXIF data from an image.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExifData {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub timestamp_original: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub software: Option<String>,
    pub orientation: Option<u16>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

/// Parse EXIF data from JPEG bytes.
pub fn parse_jpeg_exif(data: &[u8]) -> Option<ExifData> {
    // JPEG starts with FF D8.
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }

    // Scan for APP1 marker (FF E1) which contains EXIF.
    let mut pos = 2;
    while pos + 4 < data.len() {
        if data[pos] != 0xFF {
            break;
        }
        let marker = data[pos + 1];
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if pos + 2 + len > data.len() {
            break;
        }

        if marker == 0xE1 {
            // APP1 segment — check for "Exif\0\0" header.
            let segment = &data[pos + 4..pos + 2 + len];
            if segment.len() > 6 && &segment[0..6] == b"Exif\0\0" {
                let tiff_data = &segment[6..];
                return parse_tiff_exif(tiff_data);
            }
        }

        pos += 2 + len;
    }
    None
}

/// Parse TIFF-structured EXIF data.
fn parse_tiff_exif(data: &[u8]) -> Option<ExifData> {
    if data.len() < 8 {
        return None;
    }

    // Determine byte order from TIFF header.
    let big_endian = match &data[0..2] {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };

    // Verify magic number 42.
    let magic = read_u16(data, 2, big_endian)?;
    if magic != 42 {
        return None;
    }

    // Offset to IFD0.
    let ifd0_offset = read_u32(data, 4, big_endian)? as usize;
    if ifd0_offset + 2 > data.len() {
        return None;
    }

    let mut result = ExifData::default();

    // Parse IFD0 for basic tags.
    parse_ifd(data, ifd0_offset, big_endian, &mut result);

    // Check for ExifIFD pointer (tag 0x8769).
    // For GPS, check tag 0x8825.
    // These are sub-IFDs that need separate parsing.

    Some(result)
}

fn parse_ifd(data: &[u8], offset: usize, big_endian: bool, result: &mut ExifData) {
    if offset + 2 > data.len() {
        return;
    }
    let num_entries = read_u16(data, offset, big_endian).unwrap_or(0) as usize;
    let base = offset + 2;

    for i in 0..num_entries {
        let entry_off = base + i * 12;
        if entry_off + 12 > data.len() {
            break;
        }

        let tag = read_u16(data, entry_off, big_endian).unwrap_or(0);
        let type_id = read_u16(data, entry_off + 2, big_endian).unwrap_or(0);
        let count = read_u32(data, entry_off + 4, big_endian).unwrap_or(0);
        let value_offset = entry_off + 8;

        match tag {
            0x010F => {
                // Make
                result.camera_make = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x0110 => {
                // Model
                result.camera_model = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x0131 => {
                // Software
                result.software = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x0112 => {
                // Orientation
                result.orientation = read_u16(data, value_offset, big_endian);
            }
            0x0132 => {
                // DateTime (YYYY:MM:DD HH:MM:SS)
                result.timestamp_original = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x0100 => {
                // ImageWidth
                result.image_width = Some(read_u32(data, value_offset, big_endian).unwrap_or(0));
            }
            0x0101 => {
                // ImageHeight
                result.image_height = Some(read_u32(data, value_offset, big_endian).unwrap_or(0));
            }
            0x8825 => {
                // GPSInfo IFD pointer
                let gps_offset = read_u32(data, value_offset, big_endian).unwrap_or(0) as usize;
                parse_gps_ifd(data, gps_offset, big_endian, result);
            }
            _ => {}
        }
    }
}

fn parse_gps_ifd(data: &[u8], offset: usize, big_endian: bool, result: &mut ExifData) {
    if offset + 2 > data.len() {
        return;
    }
    let num_entries = read_u16(data, offset, big_endian).unwrap_or(0) as usize;
    let base = offset + 2;

    let mut lat_ref = None;
    let mut lat_rationals: Option<(u32, u32, u32, u32, u32, u32)> = None;
    let mut lon_ref = None;
    let mut lon_rationals: Option<(u32, u32, u32, u32, u32, u32)> = None;

    for i in 0..num_entries {
        let entry_off = base + i * 12;
        if entry_off + 12 > data.len() {
            break;
        }

        let tag = read_u16(data, entry_off, big_endian).unwrap_or(0);
        let type_id = read_u16(data, entry_off + 2, big_endian).unwrap_or(0);
        let count = read_u32(data, entry_off + 4, big_endian).unwrap_or(0);
        let value_offset = entry_off + 8;

        match tag {
            0x01 => {
                // GPSLatitudeRef (N/S)
                lat_ref = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x02 => {
                // GPSLatitude (3 rationals: deg, min, sec)
                lat_rationals = read_gps_rational(data, value_offset, big_endian);
            }
            0x03 => {
                // GPSLongitudeRef (E/W)
                lon_ref = read_string_value(data, type_id, count, value_offset, big_endian);
            }
            0x04 => {
                // GPSLongitude
                lon_rationals = read_gps_rational(data, value_offset, big_endian);
            }
            _ => {}
        }
    }

    // Convert rational GPS to decimal degrees.
    if let (Some(refs), Some((d_n, d_d, m_n, m_d, s_n, s_d))) = (&lat_ref, lat_rationals) {
        if d_d != 0 && m_d != 0 && s_d != 0 {
            let deg = d_n as f64 / d_d as f64;
            let min = m_n as f64 / m_d as f64;
            let sec = s_n as f64 / s_d as f64;
            let mut lat = deg + min / 60.0 + sec / 3600.0;
            if refs.contains('S') {
                lat = -lat;
            }
            result.gps_lat = Some(lat);
        }
    }

    if let (Some(refs), Some((d_n, d_d, m_n, m_d, s_n, s_d))) = (&lon_ref, lon_rationals) {
        if d_d != 0 && m_d != 0 && s_d != 0 {
            let deg = d_n as f64 / d_d as f64;
            let min = m_n as f64 / m_d as f64;
            let sec = s_n as f64 / s_d as f64;
            let mut lon = deg + min / 60.0 + sec / 3600.0;
            if refs.contains('W') {
                lon = -lon;
            }
            result.gps_lon = Some(lon);
        }
    }
}

/// Read 3 GPS rationals (deg/min/sec) from the value offset.
fn read_gps_rational(data: &[u8], offset: usize, big_endian: bool) -> Option<(u32, u32, u32, u32, u32, u32)> {
    // GPS rational is 3 UNSIGNED_RATIONAL values, each 8 bytes (numerator + denominator).
    // The offset points to the actual data (or is inline if small enough, but rationals are 8 bytes each).
    let base = offset;
    if base + 24 > data.len() {
        return None;
    }
    let d_n = read_u32(data, base, big_endian)?;
    let d_d = read_u32(data, base + 4, big_endian)?;
    let m_n = read_u32(data, base + 8, big_endian)?;
    let m_d = read_u32(data, base + 12, big_endian)?;
    let s_n = read_u32(data, base + 16, big_endian)?;
    let s_d = read_u32(data, base + 20, big_endian)?;
    Some((d_n, d_d, m_n, m_d, s_n, s_d))
}

fn read_string_value(data: &[u8], type_id: u16, count: u32, value_offset: usize, big_endian: bool) -> Option<String> {
    let offset = if count <= 4 {
        value_offset
    } else {
        read_u32(data, value_offset, big_endian)? as usize
    };
    let end = (offset + count as usize).min(data.len());
    if offset >= data.len() {
        return None;
    }
    let s = String::from_utf8_lossy(&data[offset..end]);
    let trimmed = s.trim_end_matches('\0').trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn read_u16(data: &[u8], offset: usize, big_endian: bool) -> Option<u16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(if big_endian {
        u16::from_be_bytes([data[offset], data[offset + 1]])
    } else {
        u16::from_le_bytes([data[offset], data[offset + 1]])
    })
}

fn read_u32(data: &[u8], offset: usize, big_endian: bool) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(if big_endian {
        u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
    } else {
        u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
    })
}
