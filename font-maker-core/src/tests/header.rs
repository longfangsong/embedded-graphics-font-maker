use crate::error::FontError;
use crate::format::{
    Font, MAGIC, PixelFormat, HEADER_SIZE, HEADER_SIZE_V1, GLYPH_ENTRY_SIZE, glyph_data_size,
};
use alloc::vec;
use alloc::vec::Vec;

/// Build a valid font's bytes — the independent source of truth for
/// "what a valid file looks like".
fn make_valid_font(
    pixel_format: PixelFormat,
    height: u16,
    glyphs: &[(u32, u16, &[u8])],
) -> Vec<u8> {
    make_valid_font_with_baseline(pixel_format, height, height, glyphs)
}

/// Same as [`make_valid_font`], but with an explicit baseline.
fn make_valid_font_with_baseline(
    pixel_format: PixelFormat,
    height: u16,
    baseline: u16,
    glyphs: &[(u32, u16, &[u8])],
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut cursor = HEADER_SIZE + GLYPH_ENTRY_SIZE * glyphs.len();

    // Header
    buf.extend_from_slice(&MAGIC);
    buf.push(crate::format::VERSION);
    buf.push(pixel_format as u8);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());
    buf.extend_from_slice(&baseline.to_le_bytes());

    // Per-glyph table
    for &(code, width, _) in glyphs {
        let data_len = glyph_data_size(width, height, pixel_format);
        buf.extend_from_slice(&code.to_le_bytes());
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&(cursor as u32).to_le_bytes());
        cursor += data_len;
    }

    // Glyph data
    for &(_, _, ref data) in glyphs {
        buf.extend_from_slice(data);
    }
    buf
}

/// Build a v1 font's bytes — the 12-byte header, with no `baseline` field.
fn make_v1_font(
    pixel_format: PixelFormat,
    height: u16,
    glyphs: &[(u32, u16, &[u8])],
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut cursor = HEADER_SIZE_V1 + GLYPH_ENTRY_SIZE * glyphs.len();

    buf.extend_from_slice(&MAGIC);
    buf.push(1); // v1
    buf.push(pixel_format as u8);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());

    for &(code, width, _) in glyphs {
        let data_len = glyph_data_size(width, height, pixel_format);
        buf.extend_from_slice(&code.to_le_bytes());
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&(cursor as u32).to_le_bytes());
        cursor += data_len;
    }

    for &(_, _, ref data) in glyphs {
        buf.extend_from_slice(data);
    }
    buf
}

// -- Header field validation --

#[test]
fn header_fields_are_little_endian() {
    let height: u16 = 2;
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        height,
        &[(0x41, 2, &[0u8; 4])], // 2×2 AA = 4 bytes
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.height, height);
    assert_eq!(font.header.char_count, 1); // Only 1 glyph
}

// -- Baseline --

#[test]
fn baseline_is_read_from_the_header() {
    // 18-row glyph box whose alphabetic baseline sits on row 14.
    let data = make_valid_font_with_baseline(
        PixelFormat::AntiAliased,
        18,
        14,
        &[(0x41, 2, &[0u8; 36])], // 2×18 AA
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.version, crate::format::VERSION);
    assert_eq!(font.header.height, 18);
    assert_eq!(font.header.baseline, 14);
}

#[test]
fn baseline_may_equal_height() {
    let data = make_valid_font_with_baseline(
        PixelFormat::AntiAliased,
        4,
        4,
        &[(0x41, 2, &[0u8; 8])],
    );
    assert_eq!(Font::new(&data).unwrap().header.baseline, 4);
}

#[test]
fn v1_font_reports_baseline_at_box_bottom() {
    // v1 has no baseline field — glyphs and offsets must still parse, and the
    // baseline falls back to the bottom of the glyph box.
    let data = make_v1_font(
        PixelFormat::AntiAliased,
        8,
        &[(0x41, 4, &[0xFFu8; 32]), (0x42, 4, &[0xAAu8; 32])],
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.version, 1);
    assert_eq!(font.header.height, 8);
    assert_eq!(font.header.baseline, 8);

    // The glyph table starts right after the 12-byte v1 header.
    let entry = font.get_glyph_entry(0x42).unwrap();
    assert_eq!(
        entry.data_offset as usize,
        HEADER_SIZE_V1 + 2 * GLYPH_ENTRY_SIZE + 32
    );
    assert!(font.glyph_data(&entry).iter().all(|&b| b == 0xAA));
}

#[test]
fn invalid_pixel_format_byte_returns_error() {
    // pixel_format byte at index 5 = 2 (invalid)
    let mut data = [0u8; 20];
    data[0..4].copy_from_slice(&MAGIC);
    data[4] = crate::format::VERSION;
    data[5] = 2; // invalid pixel format
    let result = Font::new(&data);
    assert!(matches!(result, Err(FontError::InvalidPixelFormat(2))));
}
