use std::vec::Vec;
use font_maker_core::format::{
    GlyphEntry, GLYPH_ENTRY_SIZE, HEADER_SIZE, MAGIC, PixelFormat, VERSION,
};

/// A detected character region in the PNG atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRegion {
    /// Starting x position in the PNG.
    pub x: u32,
    /// Width of the character in pixels.
    pub width: u16,
}

/// Scan a PNG image (given as raw pixel data with alpha) and detect character regions.
///
/// A column has "content" if any pixel in that column has alpha > 0.
/// Contiguous content columns are grouped into character regions.
///
/// # Arguments
/// * `pixels` - RGBA pixel data (width × height × 4 bytes)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
///
/// # Returns
/// A vector of character regions, sorted by x position.
pub fn detect_character_regions(pixels: &[u8], width: u32, height: u32) -> Vec<CharacterRegion> {
    let mut regions = Vec::new();
    let mut current_start: Option<u32> = None;

    for x in 0..width {
        let mut has_content = false;

        // Check all pixels in this column.
        for y in 0..height {
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < pixels.len() && pixels[idx + 3] > 0 {
                has_content = true;
                break;
            }
        }

        if has_content {
            if current_start.is_none() {
                current_start = Some(x);
            }
        } else {
            if let Some(start) = current_start.take() {
                let region_width = (x - start) as u16;
                if region_width > 0 {
                    regions.push(CharacterRegion {
                        x: start,
                        width: region_width,
                    });
                }
            }
        }
    }

    // Close any open region at the end.
    if let Some(start) = current_start {
        let region_width = (width - start) as u16;
        if region_width > 0 {
            regions.push(CharacterRegion {
                x: start,
                width: region_width,
            });
        }
    }

    regions
}

/// Zip character regions with caller-supplied code points.
///
/// # Arguments
/// * `regions` - Sorted character regions
/// * `code_points` - Code points to assign, one per region (in order)
///
/// # Returns
/// A vector of (code, region) pairs, or empty if lengths don't match.
pub fn zip_codes(regions: &[CharacterRegion], code_points: &[u32]) -> Vec<(u32, CharacterRegion)> {
    if regions.len() != code_points.len() {
        return vec![];
    }
    regions
        .iter()
        .zip(code_points.iter())
        .map(|(region, &code)| (code, region.clone()))
        .collect()
}

/// Assign sequential codes starting from the given base.
///
/// Convenience wrapper around `zip_codes` for the common sequential case.
pub fn assign_codes(regions: &[CharacterRegion], start_code: u32) -> Vec<(u32, CharacterRegion)> {
    let codes: Vec<u32> = regions
        .iter()
        .enumerate()
        .map(|(i, _)| start_code + i as u32)
        .collect();
    zip_codes(regions, &codes)
}

/// Generate binary font bytes from character regions and pixel data.
///
/// # Arguments
/// * `coded_regions` - Vector of (code, region) pairs
/// * `pixels` - RGBA pixel data
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `baseline` - Rows from the top of the glyph box down to the alphabetic
///   baseline; must be `<= height`
/// * `pixel_format` - "8bpp" or "1bpp"
///
/// # Returns
/// Binary font bytes, or an error string.
pub fn generate_binary_font(
    coded_regions: &[(u32, CharacterRegion)],
    pixels: &[u8],
    width: u32,
    height: u32,
    baseline: u16,
    pixel_format: &str,
) -> Result<Vec<u8>, String> {

    let pf = match pixel_format {
        "8bpp" => PixelFormat::AntiAliased,
        "1bpp" => PixelFormat::Monochrome,
        _ => return Err(format!("Invalid pixel format: {}", pixel_format)),
    };

    let h = height as u16;

    if baseline > h {
        return Err(format!(
            "Baseline {} is below the glyph box (height {})",
            baseline, h
        ));
    }

    // Validate pixel data length
    let expected_len = (width as usize) * (height as usize) * 4;
    if pixels.len() != expected_len {
        return Err(format!("Pixel data length mismatch: got {}, expected {}",
            pixels.len(), expected_len));
    }

    // Build glyph entries and extract pixel data.
    let mut glyphs: Vec<GlyphEntry> = Vec::new();
    let mut glyph_data: Vec<u8> = Vec::new();
    let mut data_cursor = HEADER_SIZE + GLYPH_ENTRY_SIZE * coded_regions.len();

    for &(code, ref region) in coded_regions {
        // Extract pixel data for this glyph.
        let mut raw_bits = Vec::new();
        for y in 0..height {
            for x in region.x..region.x + region.width as u32 {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < pixels.len() {
                    if pf == PixelFormat::AntiAliased {
                        raw_bits.push(pixels[idx + 3]); // Alpha channel
                    } else {
                        raw_bits.push(if pixels[idx + 3] > 128 { 1 } else { 0 });
                    }
                }
            }
        }

        let final_data = if pf == PixelFormat::Monochrome {
            pack_mono_bits(&raw_bits, region.width as usize * h as usize)
        } else {
            raw_bits
        };

        glyphs.push(GlyphEntry {
            code,
            width: region.width,
            data_offset: data_cursor as u32,
        });
        glyph_data.extend_from_slice(&final_data);
        data_cursor += final_data.len();
    }

    // Build the full binary font.
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(pf as u8);
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(&(coded_regions.len() as u32).to_le_bytes());
    buf.extend_from_slice(&baseline.to_le_bytes());

    for entry in &glyphs {
        buf.extend_from_slice(&entry.code.to_le_bytes());
        buf.extend_from_slice(&entry.width.to_le_bytes());
        buf.extend_from_slice(&entry.data_offset.to_le_bytes());
    }

    buf.extend_from_slice(&glyph_data);

    Ok(buf)
}

/// Pack raw bit values into bytes for mono format.
fn pack_mono_bits(bits: &[u8], total_pixels: usize) -> Vec<u8> {
    let byte_count = (total_pixels + 7) / 8;
    let mut bytes = vec![0u8; byte_count];
    for (i, &bit) in bits.iter().enumerate().take(total_pixels) {
        if bit > 0 {
            bytes[i / 8] |= 1 << (7 - i % 8);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn max_width(regions: &[CharacterRegion]) -> u16 {
        regions.iter().map(|r| r.width).max().unwrap_or(0)
    }

    #[test]
    fn empty_image_returns_no_regions() {
        let pixels = vec![0u8; 0];
        let regions = detect_character_regions(&pixels, 0, 0);
        assert_eq!(regions, vec![]);
    }

    #[test]
    fn all_transparent_returns_no_regions() {
        let pixels = vec![0u8; 100 * 4]; // 100x1 RGBA, all transparent
        let regions = detect_character_regions(&pixels, 100, 1);
        assert_eq!(regions, vec![]);
    }

    #[test]
    fn single_character_detected() {
        // 10x1 image with content at x=2..6
        let mut pixels = vec![0u8; 10 * 4];
        for x in 2..6 {
            pixels[x * 4 + 3] = 255; // Set alpha
        }
        let regions = detect_character_regions(&pixels, 10, 1);
        assert_eq!(
            regions,
            vec![CharacterRegion {
                x: 2,
                width: 4
            }]
        );
    }

    #[test]
    fn multiple_characters_with_spacing() {
        // 20x1 image with two characters: x=2..5 and x=10..15
        let mut pixels = vec![0u8; 20 * 4];
        for x in 2..5 {
            pixels[x * 4 + 3] = 255;
        }
        for x in 10..15 {
            pixels[x * 4 + 3] = 255;
        }
        let regions = detect_character_regions(&pixels, 20, 1);
        assert_eq!(
            regions,
            vec![
                CharacterRegion {
                    x: 2,
                    width: 3
                },
                CharacterRegion {
                    x: 10,
                    width: 5
                }
            ]
        );
    }

    #[test]
    fn adjacent_characters_no_spacing() {
        // 10x1 image with two adjacent characters: x=2..5 and x=5..8
        let mut pixels = vec![0u8; 10 * 4];
        for x in 2..8 {
            pixels[x * 4 + 3] = 255;
        }
        let regions = detect_character_regions(&pixels, 10, 1);
        // Adjacent characters with no spacing should be merged into one region
        assert_eq!(
            regions,
            vec![CharacterRegion {
                x: 2,
                width: 6
            }]
        );
    }

    #[test]
    fn code_assignment_starts_from_base() {
        let regions = vec![
            CharacterRegion { x: 0, width: 4 },
            CharacterRegion { x: 4, width: 3 },
        ];
        let coded = assign_codes(&regions, 0x41); // Start from 'A'
        assert_eq!(
            coded,
            vec![
                (0x41, CharacterRegion { x: 0, width: 4 }),
                (0x42, CharacterRegion { x: 4, width: 3 }),
            ]
        );
    }

    #[test]
    fn zip_codes_pairs_regions_with_supplied_codes() {
        let regions = vec![
            CharacterRegion { x: 0, width: 4 },
            CharacterRegion { x: 4, width: 3 },
            CharacterRegion { x: 7, width: 5 },
        ];
        // Arbitrary codes: 'Z', 'A', '0'
        let codes = vec![0x5Au32, 0x41u32, 0x30u32];
        let coded = zip_codes(&regions, &codes);
        assert_eq!(
            coded,
            vec![
                (0x5A, CharacterRegion { x: 0, width: 4 }),
                (0x41, CharacterRegion { x: 4, width: 3 }),
                (0x30, CharacterRegion { x: 7, width: 5 }),
            ]
        );
    }

    #[test]
    fn zip_codes_returns_empty_on_mismatch() {
        let regions = vec![CharacterRegion { x: 0, width: 4 }];
        let codes = vec![0x41u32, 0x42u32]; // 2 codes, 1 region
        assert!(zip_codes(&regions, &codes).is_empty());
    }

    #[test]
    fn max_width_returns_zero_for_empty() {
        let regions: Vec<CharacterRegion> = vec![];
        assert_eq!(max_width(&regions), 0);
    }

    #[test]
    fn max_width_returns_maximum() {
        let regions = vec![
            CharacterRegion { x: 0, width: 4 },
            CharacterRegion { x: 4, width: 7 },
            CharacterRegion { x: 11, width: 5 },
        ];
        assert_eq!(max_width(&regions), 7);
    }

    #[test]
    fn generate_binary_font_aa_format() {
        // Create a simple 10x4 image with one character at x=2..6
        let mut pixels = vec![0u8; 10 * 4 * 4]; // 10 wide, 4 tall, RGBA
        for y in 0..4 {
            for x in 2..6 {
                let idx = ((y * 10 + x) * 4) as usize;
                pixels[idx + 3] = 255; // Full alpha
            }
        }

        let regions = detect_character_regions(&pixels, 10, 4);
        assert_eq!(regions.len(), 1);

        let coded = assign_codes(&regions, 0x41);
        let font_bytes = generate_binary_font(&coded, &pixels, 10, 4, 3, "8bpp");
        assert!(font_bytes.is_ok());

        let bytes = font_bytes.unwrap();
        // Verify magic
        assert_eq!(&bytes[0..4], b"EFM1");
        // Verify version
        assert_eq!(bytes[4], VERSION);
        // Verify pixel format (1 = AA)
        assert_eq!(bytes[5], 1);
        // Verify height (4)
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 4);
        // Verify char_count (1)
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 1);
        // Verify baseline (3)
        assert_eq!(u16::from_le_bytes([bytes[12], bytes[13]]), 3);
    }

    #[test]
    fn generated_font_roundtrips_baseline() {
        // 10x8 image, one solid character at x=2..6
        let mut pixels = vec![0u8; 10 * 8 * 4];
        for y in 0..8 {
            for x in 2..6 {
                let idx = ((y * 10 + x) * 4) as usize;
                pixels[idx + 3] = 255;
            }
        }

        let regions = detect_character_regions(&pixels, 10, 8);
        let coded = assign_codes(&regions, 0x41);
        let bytes = generate_binary_font(&coded, &pixels, 10, 8, 6, "8bpp").unwrap();

        let font = font_maker_core::format::Font::new(&bytes).unwrap();
        assert_eq!(font.header.version, VERSION);
        assert_eq!(font.header.height, 8);
        assert_eq!(font.header.baseline, 6);
        // Glyph data still lands where the (now 14-byte) header + table end.
        let entry = font.get_glyph_entry(0x41).unwrap();
        assert_eq!(
            entry.data_offset as usize,
            HEADER_SIZE + GLYPH_ENTRY_SIZE
        );
        assert_eq!(font.glyph_data(&entry).len(), 4 * 8);
    }

    #[test]
    fn baseline_below_glyph_box_returns_error() {
        let pixels = vec![255u8; 4 * 4 * 4];
        let regions = detect_character_regions(&pixels, 4, 4);
        let coded = assign_codes(&regions, 0x41);
        let result = generate_binary_font(&coded, &pixels, 4, 4, 5, "8bpp");
        assert!(result.is_err());
    }

    #[test]
    fn generate_binary_font_mono_format() {
        // Create a simple 8x8 image with one character
        let mut pixels = vec![0u8; 8 * 4 * 8]; // 8 wide, 8 tall, RGBA
        for y in 0..8 {
            for x in 0..8 {
                let idx = ((y * 8 + x) * 4) as usize;
                pixels[idx + 3] = 255;
            }
        }

        let regions = detect_character_regions(&pixels, 8, 8);
        let coded = assign_codes(&regions, 0x41);
        let font_bytes = generate_binary_font(&coded, &pixels, 8, 8, 8, "1bpp");
        assert!(font_bytes.is_ok());

        let bytes = font_bytes.unwrap();
        // Verify magic
        assert_eq!(&bytes[0..4], b"EFM1");
        // Verify pixel format (0 = Mono)
        assert_eq!(bytes[5], 0);
    }

    #[test]
    fn invalid_pixel_format_returns_error() {
        let regions: Vec<(u32, CharacterRegion)> = vec![];
        let result = generate_binary_font(&regions, &[], 0, 0, 0, "invalid");
        assert!(result.is_err());
    }
}
