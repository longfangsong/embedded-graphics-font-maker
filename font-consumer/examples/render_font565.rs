/// Render text using Rgb565 color format.
///
/// Demonstrates FontTextStyle generic over pixel color type.
///
/// Usage: cargo run --example render_font565 -- font.bin "Hello!"
use std::env;
use std::fs;
use std::process;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{renderer::TextRenderer, Baseline, Text};
use embedded_graphics_simulator::SimulatorDisplay;
use font_consumer::{Font, FontTextStyle};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <font.bin> <text>", args[0]);
        process::exit(1);
    }

    let font_data = fs::read(&args[1]).unwrap();
    let font = Font::new(&font_data).unwrap();
    let text = &args[2];

    // Use Rgb565 (16-bit color, common in embedded displays)
    let style = FontTextStyle::new(&font, Rgb565::RED)
        .background_color(Rgb565::BLACK);

    let metrics = style.measure_string(text, Point::zero(), Baseline::Top);
    let bbox = metrics.bounding_box;
    let size = Size::new((bbox.size.width as u32 + 2).max(10), (bbox.size.height as u32 + 2).max(10));

    let mut display = SimulatorDisplay::<Rgb565>::new(size);
    display.clear(Rgb565::BLACK).ok();

    Text::with_baseline(text, Point::new(1, 1), style, Baseline::Top)
        .draw(&mut display)
        .ok();

    // SimulatorDisplay uses Rgb888 internally for PNG output
    let output_image = display.to_rgb_output_image(
        &embedded_graphics_simulator::OutputSettingsBuilder::new().scale(4).build(),
    );
    output_image.save_png("font_render_565.png").unwrap();
    println!("Saved: font_render_565.png (Rgb565)");
}
