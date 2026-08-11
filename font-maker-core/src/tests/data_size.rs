use crate::format::{Font, PixelFormat, HEADER_SIZE, GLYPH_ENTRY_SIZE};
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
    buf.extend_from_slice(&crate::format::MAGIC);
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

#[test]
fn data_size_is_width_times_height() {
    // 8×5 AA glyph → 40 bytes
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        5,
        10,
        &[(0x41, 8, &[0u8; 40])],
    );
    let font = Font::new(&data).unwrap();
    let glyph_data = font.glyph_data(0x41).unwrap();
    assert_eq!(glyph_data.len(), 40);
}

#[test]
fn mono_data_size_is_ceil_width_times_height_div_8() {
    // 4×4 mono → (16+7)/8 = 2 bytes
    let data = make_valid_font(
        PixelFormat::Monochrome,
        4,
        10,
        &[(0x41, 4, &[0xFFu8; 2])],
    );
    let font = Font::new(&data).unwrap();
    let glyph_data = font.glyph_data(0x41).unwrap();
    assert_eq!(glyph_data.len(), 2);
}

#[test]
fn mono_data_size_rounds_up() {
    // 3×3 mono → (9+7)/8 = 2 bytes
    let data = make_valid_font(
        PixelFormat::Monochrome,
        3,
        10,
        &[(0x41, 3, &[0xFFu8; 2])],
    );
    let font = Font::new(&data).unwrap();
    let glyph_data = font.glyph_data(0x41).unwrap();
    assert_eq!(glyph_data.len(), 2);
}
