use crate::error::FontError;
use crate::format::{Font, MAGIC, VERSION, PixelFormat, HEADER_SIZE, GLYPH_ENTRY_SIZE};
use alloc::vec;
use alloc::vec::Vec;

/// Build a valid font's bytes — the independent source of truth for
/// "what a valid file looks like".
fn make_valid_font(
    pixel_format: PixelFormat,
    height: u16,
    spacing: u16,
    glyphs: &[(u32, u16, &[u8])],
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut cursor = HEADER_SIZE + GLYPH_ENTRY_SIZE * glyphs.len();

    // Header
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(pixel_format as u8);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&spacing.to_le_bytes());
    buf.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());

    // Per-glyph table
    for &(code, width, _) in glyphs {
        let px_count = width as usize * height as usize;
        let data_len = match pixel_format {
            PixelFormat::AntiAliased => px_count,
            PixelFormat::Monochrome => (px_count + 7) / 8,
        };
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
    let result = Font::new(&[0u8; 13]); // Less than HEADER_SIZE (14)
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
    let data = make_valid_font(PixelFormat::AntiAliased, 16, 10, &[]);
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.char_count, 0);
    assert!(font.glyphs.is_empty());
    assert_eq!(font.header.pixel_format, PixelFormat::AntiAliased);
    assert_eq!(font.header.height, 16);
    assert_eq!(font.header.spacing, 10);
}

#[test]
fn valid_font_with_glyphs_parses() {
    let pixel_data = [0x80u8; 16]; // 4 wide × 4 tall, AA
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        8,
        &[(0x41, 4, &pixel_data)], // 'A'
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.glyphs.len(), 1);
    assert_eq!(font.glyphs[0].code, 0x41);
    assert_eq!(font.glyphs[0].width, 4);
    assert_eq!(font.glyphs[0].data_offset, 24); // 14 header + 10 table
    assert_eq!(font.glyphs[0].data_offset, data[20..24].read_le_u32());
}

#[test]
fn truncated_glyph_data_returns_error() {
    let mut data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        8,
        &[(0x41, 4, &[0x80u8; 16])],
    );
    // Shrink the file so the glyph data doesn't fit.
    data.truncate(35); // table ends at 24, data needs 16 bytes but only 11 available
    let result = Font::new(&data);
    assert!(matches!(result, Err(FontError::TruncatedGlyphData { .. })));
}

#[test]
fn invalid_data_offset_returns_error() {
    let mut data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        8,
        &[(0x41, 4, &[0x80u8; 16])],
    );
    // Verify initial state
    assert_eq!(data.len(), 40);
    // Glyph entry layout: code (4) + width (2) + data_offset (4) = 10 bytes
    // code: [14..17], width: [18..19], data_offset: [20..23]
    assert_eq!(data[20..24], [24, 0, 0, 0]); // data_offset should be 24
    
    // data_offset is at bytes 20-23. Set it to a huge value.
    data[20..24].copy_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
    assert_eq!(&data[20..24], &0xDEAD_BEEF_u32.to_le_bytes());
    
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
        10,
        &[(0x41, 4, &data_a), (0x42, 6, &data_b)],
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.glyphs.len(), 2);
    assert_eq!(font.glyphs[0].code, 0x41);
    assert_eq!(font.glyphs[0].width, 4);
    assert_eq!(font.glyphs[0].data_offset, 34); // 14 header + 2×10 table
    assert_eq!(font.glyphs[1].code, 0x42);
    assert_eq!(font.glyphs[1].width, 6);
    assert_eq!(font.glyphs[1].data_offset, 50); // 34 + 16
}
