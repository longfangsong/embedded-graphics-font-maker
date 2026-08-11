use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    Pixel,
};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay};
use std::fs;

/// Render a text string from a binary font to a preview PNG.
pub fn run_render(
    input_path: &str,
    output_path: &str,
    text: &str,
    fg_color: u32,
    bg_color: u32,
    origin_x: i32,
    origin_y: i32,
) -> Result<(), String> {
    // Read binary font.
    let font_data = fs::read(input_path)
        .map_err(|e| format!("Failed to read {}: {}", input_path, e))?;

    // Parse font.
    let font = font_consumer::Font::new(&font_data)
        .map_err(|e| format!("Failed to parse font: {:?}", e))?;

    let hdr = font.header();
    eprintln!("Font loaded: {} characters, {}x{}",
        hdr.char_count, hdr.spacing, hdr.height);

    // Calculate output size.
    // Width = sum of all character widths + (num_chars - 1) * spacing
    let text_width: i32 = if text.len() > 0 {
        let widths: i32 = text.chars().map(|ch| {
            font.glyphs().iter()
                .find(|g| g.code == ch as u32)
                .map(|g| g.width as i32)
                .unwrap_or(0)
        }).sum();
        widths + (text.len() as i32 - 1) * hdr.spacing as i32
    } else {
        0
    };
    let height = hdr.height as i32;

    // Create display.
    let size = embedded_graphics::geometry::Size::new(text_width as u32, height as u32);
    let mut display = SimulatorDisplay::<Rgb888>::new(size);

    // Fill background.
    let bg = Rgb888::new(
        ((bg_color >> 16) & 0xFF) as u8,
        ((bg_color >> 8) & 0xFF) as u8,
        (bg_color & 0xFF) as u8,
    );
    let fg = Rgb888::new(
        ((fg_color >> 16) & 0xFF) as u8,
        ((fg_color >> 8) & 0xFF) as u8,
        (fg_color & 0xFF) as u8,
    );

    Rectangle::new(Point::new(0, 0), size)
        .into_styled(PrimitiveStyle::with_fill(bg))
        .draw(&mut display)
        .map_err(|e| format!("Draw error: {:?}", e))?;

    // Render text.
    let mut x = origin_x;
    let y = origin_y;

    for ch in text.chars() {
        let code = ch as u32;
        let glyph_width = font.glyphs().iter()
            .find(|g| g.code == code)
            .map(|g| g.width as i32)
            .unwrap_or(0);
        
        if let Some(glyph_data) = font.glyph_data_by_code(code) {
            let glyph_height = font.header().height as i32;
            
            let mut pixels = Vec::new();
            for gy in 0..glyph_height {
                for gx in 0..glyph_width {
                    let px_idx = (gy * glyph_width + gx) as usize;
                    if px_idx < glyph_data.len() {
                        let alpha = glyph_data[px_idx] as f32 / 255.0;
                        if alpha > 0.0 {
                            let color = Rgb888::new(
                                (fg.r() as f32 * alpha) as u8,
                                (fg.g() as f32 * alpha) as u8,
                                (fg.b() as f32 * alpha) as u8,
                            );
                            let point = Point::new(x + gx as i32, y + gy as i32);
                            if display.bounding_box().contains(point) {
                                pixels.push(Pixel(point, color));
                            }
                        }
                    }
                }
            }
            display.draw_iter(pixels.into_iter())
                .map_err(|e| format!("Draw error: {:?}", e))?;
        }
        // Move to next character: current width + spacing
        x += glyph_width + font.header().spacing as i32;
    }

    // Save output.
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let output_image = display.to_rgb_output_image(&output_settings);
    output_image.save_png(output_path)
        .map_err(|e| format!("Failed to save PNG: {}", e))?;

    eprintln!("Preview rendered to {}", output_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert;
    use std::fs;

    #[test]
    fn render_creates_png() {
        // Create a test font.
        let width: u32 = 10;
        let height: u32 = 10;
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Fill entire image.
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                pixels[idx + 3] = 255;
            }
        }

        let regions = convert::detect_character_regions(&pixels, width, height);
        let coded = convert::assign_codes(&regions, 0x41);
        let font_bytes = convert::generate_binary_font(
            &coded,
            &pixels,
            width,
            height,
            10,
            "8bpp",
        )
        .unwrap();

        // Write font to temp file.
        let font_path = "/tmp/test_render_font.bin";
        fs::write(font_path, &font_bytes).unwrap();

        // Render.
        let output_path = "/tmp/test_render_output.png";
        let result = run_render(
            font_path,
            output_path,
            "A",
            0xFFFFFF,
            0x000000,
            0,
            0,
        );

        assert!(result.is_ok());

        // Verify output exists.
        let png_data = fs::read(output_path).unwrap();
        assert!(!png_data.is_empty());

        // Verify PNG header.
        assert_eq!(&png_data[0..4], b"\x89PNG");
    }
}
