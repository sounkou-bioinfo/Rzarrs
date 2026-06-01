// Minimal ZIP extraction using flate2 for DEFLATE decompression
// Only supports stored (method 0) and deflated (method 8) entries.
// No Zip64, no data descriptors, no encryption.

use std::io::Read;

fn read_u16le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

struct CentralDirEntry {
    compression_method: u16,
    compressed_size: usize,
    name: String,
    local_header_offset: usize,
}

fn find_eocd(data: &[u8]) -> Result<usize, String> {
    let search_start = if data.len() > 65557 {
        data.len() - 65557
    } else {
        0
    };
    for i in (search_start..data.len().saturating_sub(4)).rev() {
        if data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06] {
            return Ok(i);
        }
    }
    Err("EOCD not found".into())
}

fn parse_central_directory(
    data: &[u8],
    offset: usize,
    num_entries: usize,
) -> Result<Vec<CentralDirEntry>, String> {
    let mut entries = Vec::with_capacity(num_entries);
    let mut pos = offset;

    for _ in 0..num_entries {
        if pos + 46 > data.len() {
            return Err("unexpected end of central directory".into());
        }
        if data[pos..pos + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err("invalid central directory entry signature".into());
        }

        let _version_made = read_u16le(data, pos + 4);
        let _version_needed = read_u16le(data, pos + 6);
        let flags = read_u16le(data, pos + 8);
        let compression_method = read_u16le(data, pos + 10);
        let compressed_size = read_u32le(data, pos + 20) as usize;
        let _uncompressed_size = read_u32le(data, pos + 24) as usize;
        let name_len = read_u16le(data, pos + 28) as usize;
        let extra_len = read_u16le(data, pos + 30) as usize;
        let comment_len = read_u16le(data, pos + 32) as usize;
        let local_header_offset = read_u32le(data, pos + 42) as usize;

        if (flags & 0x08) != 0 {
            return Err("data descriptors not supported".into());
        }

        let name = String::from_utf8_lossy(&data[pos + 46..pos + 46 + name_len]).to_string();

        entries.push(CentralDirEntry {
            compression_method,
            compressed_size,
            name,
            local_header_offset,
        });

        pos += 46 + name_len + extra_len + comment_len;
    }

    Ok(entries)
}

fn read_entry_data(data: &[u8], entry: &CentralDirEntry) -> Result<Vec<u8>, String> {
    let local = entry.local_header_offset;
    if local + 30 > data.len() {
        return Err("local file header out of bounds".into());
    }
    if data[local..local + 4] != [0x50, 0x4b, 0x03, 0x04] {
        return Err("invalid local file header signature".into());
    }

    let name_len = read_u16le(data, local + 26) as usize;
    let extra_len = read_u16le(data, local + 28) as usize;
    let data_start = local + 30 + name_len + extra_len;

    if data_start + entry.compressed_size > data.len() {
        return Err("compressed data out of bounds".into());
    }

    let raw_data = &data[data_start..data_start + entry.compressed_size];

    match entry.compression_method {
        0 => Ok(raw_data.to_vec()),
        8 => {
            let mut decoder = flate2::read::DeflateDecoder::new(raw_data);
            let mut buf = Vec::new();
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| format!("DEFLATE decompression error: {e}"))?;
            Ok(buf)
        }
        other => Err(format!("unsupported compression method: {other}")),
    }
}

/// Read all entries from a ZIP file into memory.
/// Returns a list of (name, decompressed_bytes) for non-directory entries.
pub fn read_zip_entries(zip_path: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let data = std::fs::read(zip_path).map_err(|e| format!("cannot read zip file: {e}"))?;

    let eocd_offset = find_eocd(&data)?;
    let num_entries = read_u16le(&data, eocd_offset + 10) as usize;
    let cd_offset = read_u32le(&data, eocd_offset + 16) as usize;

    let entries = parse_central_directory(&data, cd_offset, num_entries)?;

    let mut result = Vec::with_capacity(num_entries);
    for entry in &entries {
        if entry.name.ends_with('/') || entry.name.is_empty() {
            continue;
        }
        let entry_data = read_entry_data(&data, entry)?;
        result.push((entry.name.clone(), entry_data));
    }

    Ok(result)
}
