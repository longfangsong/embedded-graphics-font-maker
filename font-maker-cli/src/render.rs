use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::*,
    text::{renderer::TextRenderer, Baseline, Text},
};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay};
use font_consumer::FontTextStyle;
use std::fs;

/// Render a text string from a binary font to a preview PNG.
/// Delegates to font-consumer's FontTextStyle (TextRenderer).
pub fn run_render(
    input_path: &str,
    output_path: &str,
    text: &str,
    fg_color: u32,
    bg_color: u32,
    origin_x: i32,
    origin_y: i32,
) -> Result<(), String> {
    let font_data = fs::read(input_path)
        .map_err(|e| format!("Failed to read {}: {}", input_path, e))?;

    let font = font_maker_core::format::Font::new(&font_data)
        .map_err(|e| format!("Failed to parse font: {:?}", e))?;

    eprintln!("Font loaded: v{}, {} characters, height={}, baseline={}",
        font.header.version, font.header.char_count, font.header.height, font.header.baseline);

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

    let style = FontTextStyle::new(&font, fg).background_color(bg);
    let metrics = style.measure_string(text, Point::zero(), Baseline::Top);
    let width = (metrics.bounding_box.size.width as i32 + origin_x).max(1);
    let height = font.header.height as i32;

    let size = Size::new(width as u32, height as u32);
    let mut display = SimulatorDisplay::<Rgb888>::new(size);
    display.clear(bg).map_err(|e| format!("Draw error: {:?}", e))?;

    Text::with_baseline(
        text,
        Point::new(origin_x, origin_y),
        style,
        Baseline::Top,
    )
    .draw(&mut display)
    .map_err(|e| format!("Draw error: {:?}", e))?;

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
            height as u16,
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
