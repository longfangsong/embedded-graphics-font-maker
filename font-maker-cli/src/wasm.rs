//! WASM interface for font conversion.

use wasm_bindgen::prelude::*;
use crate::convert::{CharacterRegion, zip_codes, generate_binary_font};

/// Initialize WASM.
#[wasm_bindgen(start)]
fn wasm_start() {
    console_error_panic_hook::set_once();
}

/// Convert RGBA pixel data (from canvas) to binary font.
///
/// `baseline` is the canvas row the glyphs were drawn on (`ctx.fillText` y with
/// `textBaseline = 'alphabetic'`), counted from the top of the atlas.
#[wasm_bindgen]
pub fn convert_atlas(
    pixels: &[u8],
    width: u32,
    height: u32,
    codepoints: &[u32],
    char_widths: &[u16],
    char_positions: &[u32],
    baseline: u16,
) -> Vec<u8> {
    // Validate inputs
    let n = codepoints.len();
    if n == 0 || char_widths.is_empty() || char_positions.is_empty() {
        return vec![];
    }
    if char_widths.len() != n || char_positions.len() != n {
        return vec![];
    }

    // Build regions
    let mut regions = Vec::new();
    for i in 0..n {
        let x = char_positions[i];
        let rw = char_widths[i] as u32;
        if x + rw > width {
            return vec![];
        }
        regions.push(CharacterRegion { x, width: rw as u16 });
    }

    let coded_regions = zip_codes(&regions, codepoints);
    if coded_regions.is_empty() {
        return vec![];
    }

    match generate_binary_font(&coded_regions, pixels, width, height, baseline, "8bpp") {
        Ok(b) => b,
        Err(e) => {
            web_sys::console::error_1(&e.into());
            vec![]
        }
    }
}
