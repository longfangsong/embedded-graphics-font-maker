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
fn roundtrip_preserves_data() {
    let data_a = [0xAAu8; 12]; // 4×3 AA
    let data_b = [0x55u8; 6];  // 2×3 AA
    let original = make_valid_font(
        PixelFormat::AntiAliased,
        3,
        8,
        &[(0x41, 4, &data_a), (0x42, 2, &data_b)],
    );
    let font = Font::new(&original).unwrap();
    let serialized = font.to_bytes();
    assert_eq!(serialized, original);

    // Re-parse the serialized bytes.
    let font2 = Font::new(&serialized).unwrap();
    assert_eq!(font2.header, font.header);
    assert_eq!(font2.glyphs, font.glyphs);
    assert_eq!(font2.glyph_data(0x41), Some(&data_a[..]));
    assert_eq!(font2.glyph_data(0x42), Some(&data_b[..]));
}

#[test]
fn roundtrip_mono_preserves_data() {
    // 4×4 mono = (16+7)/8 = 2 bytes
    let mono_data = [0xABu8; 2]; // covers various bit patterns
    let original = make_valid_font(
        PixelFormat::Monochrome,
        4,
        8,
        &[(0x41, 4, &mono_data)],
    );
    let font = Font::new(&original).unwrap();
    let serialized = font.to_bytes();
    assert_eq!(serialized, original);
    let font2 = Font::new(&serialized).unwrap();
    assert_eq!(font2.glyph_data(0x41), Some(&mono_data[..]));
}

#[test]
fn glyph_data_returns_none_for_unknown_code() {
    let data = make_valid_font(
        PixelFormat::AntiAliased,
        4,
        8,
        &[(0x41, 4, &[0x80u8; 16])],
    );
    let font = Font::new(&data).unwrap();
    assert_eq!(font.glyph_data(0x42), None);
}
