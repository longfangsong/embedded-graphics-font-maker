/// Render text using a binary font file.
///
/// Outputs PNG by default (8x scaled). Use --window to open SDL2 window.
///
/// Usage:
///   cargo run --example render_font -- font.bin "Hello!" [--fg-color RRGGBB] [--bg-color RRGGBB] [--char-spacing N] [--output path.png]
///
/// Examples:
///   cargo run --example render_font -- myfont.bin "Hello!" --fg-color FFFFFF --bg-color 000000
///   cargo run --example render_font -- myfont.bin "Hello!" --char-spacing 2 --output out.png
///   cargo run --example render_font -- myfont.bin "Test" --window
///
/// Colors: hex RRGGBB or RGB565 (RRGGBB format). Default: white on transparent.
use std::env;
use std::fs;
use std::process;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{renderer::TextRenderer, Baseline, Text};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent};
use font_consumer::FontTextStyle;
use font_maker_core::format::Font as CoreFont;

fn parse_color(arg: &str) -> Option<Rgb888> {
    let hex = arg.trim_start_matches('#');
    // RGB888: 3 bytes (e.g., FFFFFF)
    {
        let bytes = hex::decode(hex).ok()?;
        if bytes.len() == 3 {
            return Some(Rgb888::new(bytes[0], bytes[1], bytes[2]));
        }
    }
    // RGB565: 2 bytes (e.g., FFFF, 7BEF)
    {
        let bytes = hex::decode(hex).ok()?;
        if bytes.len() == 2 {
            let v = u16::from_be_bytes(bytes.try_into().ok()?);
            let r = ((v >> 11) & 0x1F) as u8;
            let g = ((v >> 5) & 0x3F) as u8;
            let b = (v & 0x1F) as u8;
            let r8 = (r << 3) | (r >> 2);
            let g8 = (g << 2) | (g >> 4);
            let b8 = (b << 3) | (b >> 2);
            return Some(Rgb888::new(r8, g8, b8));
        }
    }
    None
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <font.bin> <text> [--fg-color RRGGBB] [--bg-color RRGGBB] [--char-spacing N] [--output <path.png>] [--window]", args[0]);
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  {} myfont.bin \"Hello!\" --fg-color FFFFFF --output out.png", args[0]);
        eprintln!("  {} myfont.bin \"Hello!\" --fg-color 00FF00 --bg-color 111111 --char-spacing 2", args[0]);
        eprintln!("  {} myfont.bin \"Test\" --window", args[0]);
        eprintln!("(Default: saves to font_render.png)");
        process::exit(1);
    }

    let font_path = &args[1];
    let text = &args[2];

    let mut fg_color = Rgb888::WHITE;
    let mut bg_color: Option<Rgb888> = None;
    let mut char_spacing: Option<u32> = None;
    let mut output_path: Option<String> = None;
    let mut use_window = false;
    let mut i = 3;

    while i < args.len() {
        match args[i].as_str() {
            "--fg-color" => {
                i += 1;
                if i < args.len() {
                    fg_color = parse_color(&args[i]).unwrap_or_else(|| {
                        eprintln!("Invalid fg-color: {}, using WHITE", args[i]);
                        Rgb888::WHITE
                    });
                }
            }
            "--bg-color" => {
                i += 1;
                if i < args.len() {
                    bg_color = parse_color(&args[i]);
                }
            }
            "--char-spacing" => {
                i += 1;
                if i < args.len() {
                    char_spacing = args[i].parse().ok();
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() {
                    output_path = Some(args[i].clone());
                }
            }
            "--window" => {
                use_window = true;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let font_data = fs::read(font_path).unwrap_or_else(|e| {
        eprintln!("Failed to read font file '{}': {}", font_path, e);
        process::exit(1);
    });

    let font = CoreFont::new(&font_data).unwrap_or_else(|e| {
        eprintln!("Failed to load font: {}", e);
        process::exit(1);
    });

    // Calculate display size from text metrics
    let style = if let Some(sp) = char_spacing {
        FontTextStyle::new(&font, fg_color)
            .char_spacing(sp)
            .background_color(bg_color.unwrap_or(Rgb888::BLACK))
    } else {
        FontTextStyle::new(&font, fg_color)
            .background_color(bg_color.unwrap_or(Rgb888::BLACK))
    };
    let metrics = style.measure_string(text, Point::zero(), Baseline::Top);

    let bbox = metrics.bounding_box;
    let width = (bbox.size.width as u32 + 2).max(10);
    let height = (bbox.size.height as u32 + 2).max(10);

    let mut display = SimulatorDisplay::<Rgb888>::new(Size::new(width, height));

    // Fill background
    display.clear(bg_color.unwrap_or(Rgb888::BLACK)).ok();

    // Render text (offset by 1 pixel for padding)
    Text::with_baseline(
        text,
        Point::new(1, 1),
        style,
        Baseline::Top,
    )
    .draw(&mut display)
    .ok();

    let output_settings = OutputSettingsBuilder::new().scale(8).build();

    if use_window {
        use embedded_graphics_simulator::Window;
        let mut window = Window::new("Font Consumer Demo", &output_settings);
        window.show_static(&display);
        println!("Press any key to exit...");

        // Block until key press
        loop {
            for event in window.events() {
                if let SimulatorEvent::KeyDown { .. } | SimulatorEvent::KeyUp { .. } = event {
                    return;
                }
            }
        }
    } else {
        let path = output_path.unwrap_or_else(|| "font_render.png".to_string());
        let output_image = display.to_rgb_output_image(&output_settings);
        output_image.save_png(&path).unwrap_or_else(|e| {
            eprintln!("Failed to save image: {}", e);
            process::exit(1);
        });
        println!("Saved: {}", path);
    }
}
