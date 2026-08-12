use crate::error::FontError;
use crate::format::{Font, MAGIC, VERSION, PixelFormat, HEADER_SIZE, GLYPH_ENTRY_SIZE, glyph_data_size};
use alloc::vec;
use alloc::vec::Vec;

/// Build a valid font's bytes — the independent source of truth for
/// "what a valid file looks like".
fn make_valid_font(
    pixel_format: PixelFormat,
    height: u16,
    glyphs: &[(u32, u16, &[u8])],
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut cursor = HEADER_SIZE + GLYPH_ENTRY_SIZE * glyphs.len();

    // Header
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(pixel_format as u8);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());

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

// Helper: read_le_u32 on a byte slice for tests.
trait ReadLeU32Ext {
    fn read_le_u32(&self) -> u32;
}
impl ReadLeU32Ext for [u8] {
    fn read_le_u32(&self) -> u32 {
        u32::from_le_bytes([self[0], self[1], self[2], self[3]])
    }
}

// -- Parser tests --

#[test]
fn empty_file_returns_truncated() {
    let result = Font::new(&[]);
    assert_eq!(result, Err(FontError::TruncatedFile));
}

#[test]
fn truncated_header_returns_truncated() {
    let result = Font::new(&[0u8; 11]); // Less than HEADER_SIZE (12)
    assert_eq!(result, Err(FontError::TruncatedFile));
}

#[test]
fn invalid_magic_returns_error() {
    let data = [0u8; 20]; // wrong magic
    let result = Font::new(&data);
    assert_eq!(result, Err(FontError::InvalidMagic));
}

#[test]
fn unsupported_version_returns_error() {
    let mut data = [0u8; 20];
    data[0..4].copy_from_slice(&MAGIC);
    data[4] = 99; // unsupported version
    let result = Font::new(&data);
    assert_eq!(result, Err(FontError::UnsupportedVersion(99)));
}

#[test]
fn empty_font_parses_successfully() {
    let data = make_valid_font(PixelFormat::AntiAliased, 16, &[]);
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.char_count, 0);
    assert!(font.get_glyph_entry(0x41).is_none());
    assert_eq!(font.header.pixel_format, PixelFormat::AntiAliased);
    assert_eq!(font.header.height, 16);
}

#[test]
fn valid_font_with_glyphs_parses() {
    let pixel_data = [0x80u8; 16]; // 4 wide × 4 tall, AA
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        &[(0x41, 4, &pixel_data)], // 'A'
    );
    let font = Font::new(&data).unwrap();
    let entry = font.get_glyph_entry(0x41).unwrap();
    assert_eq!(entry.code, 0x41);
    assert_eq!(entry.width, 4);
    assert_eq!(entry.data_offset, 22); // 12 header + 10 table
    assert_eq!(entry.data_offset, data[18..22].read_le_u32());
}

#[test]
fn truncated_glyph_data_returns_error() {
    let mut data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        &[(0x41, 4, &[0x80u8; 16])],
    );
    // Shrink the file so the glyph data doesn't fit.
    data.truncate(33); // table ends at 22, data needs 16 bytes but only 11 available
    let result = Font::new(&data);
    assert!(matches!(result, Err(FontError::TruncatedGlyphData { .. })));
}

#[test]
fn invalid_data_offset_returns_error() {
    let mut data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        &[(0x41, 4, &[0x80u8; 16])],
    );
    // Verify initial state: header(12) + table(10) + data(16) = 38
    assert_eq!(data.len(), 38);
    // Glyph entry layout: code (4) + width (2) + data_offset (4) = 10 bytes
    // code: [12..15], width: [16..17], data_offset: [18..21]
    assert_eq!(data[18..22], [22, 0, 0, 0]); // data_offset should be 22
    
    // data_offset is at bytes 18-21. Set it to a huge value.
    data[18..22].copy_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
    assert_eq!(&data[18..22], &0xDEAD_BEEF_u32.to_le_bytes());
    
    let result = Font::new(&data);
    // This should fail because data_offset is huge
    assert!(result.is_err(), "Expected error but got Ok");
}

#[test]
fn multiple_glyphs_parse_correctly() {
    let data_a = [0xFFu8; 16]; // 4×4 AA
    let data_b = [0x0Fu8; 24]; // 6×4 AA
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        &[(0x41, 4, &data_a), (0x42, 6, &data_b)],
    );
    let font = Font::new(&data).unwrap();
    let entry_a = font.get_glyph_entry(0x41).unwrap();
    let entry_b = font.get_glyph_entry(0x42).unwrap();
    assert_eq!(entry_a.code, 0x41);
    assert_eq!(entry_a.width, 4);
    assert_eq!(entry_a.data_offset, 32); // 12 header + 2×10 table
    assert_eq!(entry_b.code, 0x42);
    assert_eq!(entry_b.width, 6);
    assert_eq!(entry_b.data_offset, 48); // 32 + 16
}

#[test]
fn get_glyph_entry_scans_by_code_point() {
    // Three glyphs: scan must pass first two to find third.
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        2,
        &[
            (0x41, 4, &[0xFFu8; 8]),   // 'A'
            (0x42, 4, &[0xAAu8; 8]),   // 'B'
            (0x43, 4, &[0x55u8; 8]),   // 'C'
        ],
    );
    let font = Font::new(&data).unwrap();

    // First entry — found immediately.
    assert!(font.get_glyph_entry(0x41).is_some());

    // Last entry — must scan past first two.
    let entry_c = font.get_glyph_entry(0x43).unwrap();
    assert_eq!(entry_c.code, 0x43);
    assert_eq!(entry_c.width, 4);

    // Non-existent code — full scan, not found.
    assert!(font.get_glyph_entry(0x44).is_none());
}

#[test]
fn new_fast_skips_glyph_validation() {
    // Build a font with an invalid data_offset (past EOF).
    let mut buf = make_valid_font(PixelFormat::AntiAliased, 8, &[(0x41, 4, &[0xFFu8; 8])]);
    // Corrupt the data_offset to point past EOF.
    let entry_offset = HEADER_SIZE + 6; // data_offset starts at byte 6 of first entry.
    buf[entry_offset..entry_offset + 4].copy_from_slice(&(999_u32).to_le_bytes());

    // new_fast should succeed (header + table are valid).
    let font = Font::new_fast(&buf);
    assert!(font.is_ok());

    // But validate_glyphs should catch the bad offset.
    assert!(matches!(
        font.unwrap().validate_glyphs(),
        Err(FontError::InvalidDataOffset { .. })
    ));

    // new (full validation) should fail.
    assert!(matches!(
        Font::new(&buf),
        Err(FontError::InvalidDataOffset { .. })
    ));
}

#[test]
fn validate_glyphs_ok_on_valid_font() {
    // 4×8 AA = 32 bytes per glyph
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        8,
        &[(0x41, 4, &[0xFFu8; 32]), (0x42, 4, &[0xAAu8; 32])],
    );
    let font = Font::new_fast(&data).unwrap();
    assert!(font.validate_glyphs().is_ok());
}
