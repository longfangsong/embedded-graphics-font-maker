use clap::{Parser, Subcommand};
use std::fs;
use std::io::Read;

use font_maker_cli::convert;
use font_maker_cli::render;

#[derive(Parser)]
#[command(name = "font-maker-cli")]
#[command(about = "Convert PNG character atlas to binary font and render previews")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert a PNG character atlas to a binary font file
    Convert {
        /// Input PNG file path
        #[arg(short, long)]
        input: String,

        /// Output binary font file path
        #[arg(short, long)]
        output: String,

        /// Starting ASCII code (default: 0x20 space). Ignored if --chars is provided.
        #[arg(long, default_value = "32")]
        start_code: u32,

        /// Characters in the atlas, in left-to-right order. E.g. "A-Za-z0-9"
        /// If not provided, characters are assigned sequentially starting at --start-code.
        #[arg(long)]
        chars: Option<String>,

        /// Pixel format: 8bpp or 1bpp (default: 8bpp)
        #[arg(long, default_value = "8bpp")]
        format: String,
    },

    /// Render a test string from a binary font to a preview PNG
    Render {
        /// Input binary font file path
        #[arg(short, long)]
        input: String,

        /// Output preview PNG file path
        #[arg(short, long)]
        output: String,

        /// Text string to render
        #[arg(short, long)]
        text: String,

        /// Foreground color (hex: 0xRRGGBB or decimal)
        #[arg(long, default_value = "255")]
        fg_color: String,

        /// Background color (hex: 0xRRGGBB or decimal)
        #[arg(long, default_value = "0")]
        bg_color: String,

        /// Origin X position
        #[arg(long, default_value = "0")]
        origin_x: i32,

        /// Origin Y position
        #[arg(long, default_value = "0")]
        origin_y: i32,
    },
}

fn parse_color(s: &str) -> Option<u32> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("#")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert {
            input,
            output,
            start_code,
            chars,
            format,
        } => {
            if let Err(e) = run_convert(&input, &output, start_code, chars.as_deref(), &format) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Render {
            input,
            output,
            text,
            fg_color,
            bg_color,
            origin_x,
            origin_y,
        } => {
            let fg = parse_color(&fg_color).unwrap_or(0xFFFFFF);
            let bg = parse_color(&bg_color).unwrap_or(0x000000);
            if let Err(e) = render::run_render(&input, &output, &text, fg, bg, origin_x, origin_y) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn run_convert(
    input_path: &str,
    output_path: &str,
    start_code: u32,
    chars: Option<&str>,
    format: &str,
) -> Result<(), String> {
    // Read PNG file.
    let mut file =
        fs::File::open(input_path).map_err(|e| format!("Failed to open {}: {}", input_path, e))?;
    let mut png_data = Vec::new();
    file.read_to_end(&mut png_data)
        .map_err(|e| format!("Failed to read {}: {}", input_path, e))?;

    // Decode PNG.
    let decoder = png::Decoder::new(&png_data[..]);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("Failed to decode PNG: {}", e))?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let color_type = info.color_type;

    // Only support RGBA 8-bit.
    if color_type != png::ColorType::Rgba {
        return Err("Only RGBA 8-bit PNG is supported".to_string());
    }

    let mut buffer = vec![0u8; width as usize * height as usize * 4];
    reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("Failed to read PNG frame: {}", e))?;

    println!("PNG loaded: {}x{}", width, height);

    // Detect character regions.
    let regions = convert::detect_character_regions(&buffer, width, height);
    println!("Detected {} character regions", regions.len());

    if regions.is_empty() {
        return Err("No characters detected in PNG".to_string());
    }

    // Assign codes.
    let coded_regions = if let Some(chars_str) = chars {
        let code_points: Vec<u32> = chars_str
            .chars()
            .map(|c| c as u32)
            .collect();
        if code_points.len() != regions.len() {
            return Err(format!(
                "Character count mismatch: --chars has {} chars but {} regions detected",
                code_points.len(),
                regions.len()
            ));
        }
        convert::zip_codes(&regions, &code_points)
    } else {
        convert::assign_codes(&regions, start_code)
    };

    // Generate binary font.
    let font_bytes = convert::generate_binary_font(&coded_regions, &buffer, width, height, format)?;

    // Write output file.
    fs::write(output_path, &font_bytes)
        .map_err(|e| format!("Failed to write {}: {}", output_path, e))?;

    println!(
        "Binary font written to {} ({} bytes)",
        output_path,
        font_bytes.len()
    );
    Ok(())
}
