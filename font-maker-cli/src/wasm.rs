//! WASM interface for font conversion.
//!
//! Browser renders TTF chars to canvas → extracts RGBA pixels → calls this module → downloads .bin

use wasm_bindgen::prelude::*;
use crate::convert::{CharacterRegion, generate_binary_font};

/// Convert RGBA pixel data (from canvas) to binary font.
#[wasm_bindgen]
pub fn convert_atlas(
    pixels: &[u8],
    width: u32,
    height: u32,
    spacing: u16,
    codepoints: &[u32],
    char_widths: &[u16],
    char_positions: &[u32],
) -> Vec<u8> {
    if codepoints.is_empty() || char_widths.is_empty() || char_positions.is_empty() {
        return vec![];
    }
    if codepoints.len() != char_widths.len() || codepoints.len() != char_positions.len() {
        return vec![];
    }

    let mut regions: Vec<CharacterRegion> = Vec::new();
    for (&x, &rw) in char_positions.iter().zip(char_widths.iter()) {
        if x + rw as u32 > width {
            return vec![];
        }
        regions.push(CharacterRegion { x, width: rw });
    }

    let coded_regions: Vec<(u32, CharacterRegion)> = regions
        .into_iter()
        .zip(codepoints.iter())
        .map(|(region, &code)| (code, region))
        .collect();

    let format = "8bpp";
    match generate_binary_font(&coded_regions, pixels, width, height, spacing, format) {
        Ok(b) => b,
        Err(_) => vec![],
    }
}
