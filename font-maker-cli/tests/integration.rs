#[test]
fn convert_png_to_binary() {
    // Create a test PNG with two characters.
    let width: u32 = 20;
    let height: u32 = 10;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    // Draw 'A' at x=2..7
    for y in 0..height {
        for x in 2..7 {
            let idx = ((y * width + x) * 4) as usize;
            pixels[idx + 3] = 255; // Alpha
        }
    }

    // Draw 'B' at x=12..17
    for y in 0..height {
        for x in 12..17 {
            let idx = ((y * width + x) * 4) as usize;
            pixels[idx + 3] = 255;
        }
    }

    // Detect regions.
    let regions = font_maker_cli::convert::detect_character_regions(&pixels, width, height);
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].x, 2);
    assert_eq!(regions[0].width, 5);
    assert_eq!(regions[1].x, 12);
    assert_eq!(regions[1].width, 5);

    // Assign codes.
    let coded = font_maker_cli::convert::assign_codes(&regions, 0x41);
    assert_eq!(coded[0].0, 0x41);
    assert_eq!(coded[1].0, 0x42);

    // Generate binary font.
    let font_bytes = font_maker_cli::convert::generate_binary_font(
        &coded,
        &pixels,
        width,
        height,
        "8bpp",
    )
    .unwrap();

    // Verify magic.
    assert_eq!(&font_bytes[0..4], b"EFM1");

    // Verify header.
    let font = font_maker_core::format::Font::new(&font_bytes).unwrap();
    let hdr = &font.header;
    assert_eq!(hdr.char_count, 2);
    assert_eq!(hdr.height, 10);
    assert_eq!(hdr.pixel_format, font_maker_core::format::PixelFormat::AntiAliased);

    // Verify glyphs.
    let entry_a = font.get_glyph_entry(0x41).unwrap();
    let entry_b = font.get_glyph_entry(0x42).unwrap();
    assert_eq!(entry_a.code, 0x41);
    assert_eq!(entry_a.width, 5);
    assert_eq!(entry_b.code, 0x42);
    assert_eq!(entry_b.width, 5);

    // Verify glyph data.
    let data_a = font.glyph_data(&entry_a);
    assert_eq!(data_a.len(), 50); // 5×10
    assert!(data_a.iter().all(|&b| b == 255));

    let data_b = font.glyph_data(&entry_b);
    assert_eq!(data_b.len(), 50);
    assert!(data_b.iter().all(|&b| b == 255));
}

#[test]
fn convert_mono_format() {
    let width: u32 = 8;
    let height: u32 = 8;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    // Fill entire image.
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            pixels[idx + 3] = 255;
        }
    }

    let regions = font_maker_cli::convert::detect_character_regions(&pixels, width, height);
    assert_eq!(regions.len(), 1);

    let coded = font_maker_cli::convert::assign_codes(&regions, 0x41);
    let font_bytes = font_maker_cli::convert::generate_binary_font(
        &coded,
        &pixels,
        width,
        height,
        "1bpp",
    )
    .unwrap();

    let font = font_maker_core::format::Font::new(&font_bytes).unwrap();
    assert_eq!(font.header.pixel_format, font_maker_core::format::PixelFormat::Monochrome);

    // 8×8 mono = 8 bytes.
    let entry = font.get_glyph_entry(0x41).unwrap();
    let data = font.glyph_data(&entry);
    assert_eq!(data.len(), 8);
}

#[test]
fn render_creates_png() {
    use std::fs;

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

    let regions = font_maker_cli::convert::detect_character_regions(&pixels, width, height);
    let coded = font_maker_cli::convert::assign_codes(&regions, 0x41);
    let font_bytes = font_maker_cli::convert::generate_binary_font(
        &coded,
        &pixels,
        width,
        height,
        "8bpp",
    )
    .unwrap();

    // Write font to temp file.
    let font_path = "/tmp/test_render_font.bin";
    fs::write(font_path, &font_bytes).unwrap();

    // Render.
    let output_path = "/tmp/test_render_output.png";
    let result = font_maker_cli::render::run_render(
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
