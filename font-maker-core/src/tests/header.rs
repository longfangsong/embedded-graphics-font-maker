use crate::error::FontError;
use crate::format::{Font, MAGIC, PixelFormat, HEADER_SIZE, GLYPH_ENTRY_SIZE};
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
    buf.push(crate::format::VERSION);
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

// -- Header field validation --

#[test]
fn header_fields_are_little_endian() {
    let height: u16 = 2;
    let spacing: u16 = 0x0403;
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        height,
        spacing,
        &[(0x41, 2, &[0u8; 4])], // 2×2 AA = 4 bytes
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.header.height, height);
    assert_eq!(font.header.spacing, spacing);
    assert_eq!(font.header.char_count, 1); // Only 1 glyph
}

#[test]
fn invalid_pixel_format_byte_returns_error() {
    // pixel_format byte at index 5 = 2 (invalid)
    let mut data = [0u8; 20];
    data[0..4].copy_from_slice(&MAGIC);
    data[4] = crate::format::VERSION;
    data[5] = 2; // invalid pixel format
    let result = Font::new(&data);
    assert!(matches!(result, Err(FontError::UnsupportedVersion(2))));
}
