#![no_std]
extern crate alloc;

use alloc::vec::Vec;

fn blend_channel(bg: u8, fg: u8, alpha: u8) -> u8 {
    let a = alpha as u16;
    ((bg as u16 * (255 - a) + fg as u16 * a) / 255) as u8
}

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{PixelColor, Rgb888, RgbColor},
    primitives::Rectangle,
    text::{
        renderer::{TextMetrics, TextRenderer},
        Baseline,
    },
    Pixel,
};
use font_maker_core::format::{Font as CoreFont, GlyphEntry, Header, PixelFormat};
use font_maker_core::error::FontError;

/// Character bounds in a text layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterBounds {
    /// Character's pixel width (content only).
    pub width: u16,
    /// Font's header height.
    pub height: u16,
}

/// Text style for rendering with a binary font.
///
/// Generic over pixel color type (Rgb888, Rgb565, etc.).
/// Implements [`TextRenderer`] to integrate with embedded-graphics text infrastructure.
#[derive(Debug, Clone)]
pub struct FontTextStyle<'a, C> {
    /// Font reference.
    pub font: &'a Font<'a>,
    /// Text color.
    pub text_color: C,
    /// Background color (optional).
    pub background_color: Option<C>,
    /// Additional character spacing override. Uses font header spacing if 0.
    pub char_spacing: Option<u32>,
}

impl<'a, C: PixelColor + From<Rgb888> + Into<Rgb888>> FontTextStyle<'a, C> {
    /// Create a new text style with the given font and text color.
    pub fn new(font: &'a Font<'a>, text_color: C) -> Self {
        Self {
            font,
            text_color,
            background_color: None,
            char_spacing: None,
        }
    }

    /// Set additional character spacing.
    pub fn char_spacing(mut self, spacing: u32) -> Self {
        self.char_spacing = Some(spacing);
        self
    }

    /// Set background color.
    pub fn background_color(mut self, color: C) -> Self {
        self.background_color = Some(color);
        self
    }

    /// Draw a single character at the given position.
    fn draw_char<D>(&self, c: char, pos: Point, target: &mut D) -> Result<u32, D::Error>
    where
        D: DrawTarget<Color = C>,
    {
        let glyph = self.font.get_glyph_entry(c).expect("glyph not found");
        let data = self.font.glyph_data(glyph);
        let width = glyph.width as usize;
        let height = self.font.header().height as usize;

        let mut pixels = Vec::new();

        match self.font.pixel_format() {
            PixelFormat::AntiAliased => {
                let fg8: Rgb888 = self.text_color.into();
                let bg8: Rgb888 = self.background_color.map(Into::into).unwrap_or(Rgb888::BLACK);
                for y in 0..height {
                    for x in 0..width {
                        let idx = y * width + x;
                        if idx >= data.len() {
                            break;
                        }
                        let alpha = data[idx];
                        if alpha > 0 {
                            let blended8 = Rgb888::new(
                                blend_channel(bg8.r(), fg8.r(), alpha),
                                blend_channel(bg8.g(), fg8.g(), alpha),
                                blend_channel(bg8.b(), fg8.b(), alpha),
                            );
                            pixels.push(Pixel(
                                (pos.x + x as i32, pos.y + y as i32).into(),
                                C::from(blended8),
                            ));
                        }
                    }
                }
            }
            PixelFormat::Monochrome => {
                for y in 0..height {
                    for x in 0..width {
                        let bit_idx = y * width + x;
                        let byte_idx = bit_idx / 8;
                        let bit = 7 - (bit_idx % 8);
                        if byte_idx < data.len() {
                            let fg = (data[byte_idx] >> bit) & 1 != 0;
                            if fg {
                                pixels.push(Pixel(
                                    (pos.x + x as i32, pos.y + y as i32).into(),
                                    self.text_color,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if !pixels.is_empty() {
            target.draw_iter(pixels.into_iter())?;
        }

        Ok(glyph.width as u32)
    }
}

/// A loaded binary font.
///
/// Wraps the parsed font data and exposes consumer-side APIs.
#[derive(Debug, Clone)]
pub struct Font<'a> {
    inner: CoreFont<'a>,
}

impl<C: PixelColor + From<Rgb888> + Into<Rgb888>> TextRenderer for FontTextStyle<'_, C> {
    type Color = C;

    fn draw_string<D>(
        &self,
        text: &str,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let height = self.font.header().height as i32;
        let y_offset = match baseline {
            Baseline::Top => 0,
            Baseline::Alphabetic => 0,
            Baseline::Middle => -(height / 2),
            Baseline::Bottom => -height,
        };
        let mut x = position.x;
        let y = position.y + y_offset;
        let spacing = self.char_spacing.unwrap_or(self.font.header().spacing as u32) as i32;

        for c in text.chars() {
            if let Some(entry) = self.font.get_glyph_entry(c) {
                self.draw_char(c, Point::new(x, y), target)?;
                x += entry.width as i32 + spacing;
            }
        }

        Ok(Point::new(x, position.y))
    }

    fn draw_whitespace<D>(
        &self,
        width: u32,
        position: Point,
        _baseline: Baseline,
        _target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        Ok(Point::new(position.x + width as i32, position.y))
    }

    fn measure_string(&self, text: &str, position: Point, _baseline: Baseline) -> TextMetrics {
        let mut width: u32 = 0;
        let spacing = self.char_spacing.unwrap_or(self.font.header().spacing as u32);

        for (i, c) in text.chars().enumerate() {
            if let Some(entry) = self.font.get_glyph_entry(c) {
                width += entry.width as u32;
                if i < text.chars().count() - 1 {
                    width += spacing;
                }
            }
        }

        let height = self.font.header().height as u32;
        let bb = Rectangle::new(position, Size::new(width, height));
        let next_pos = Point::new(position.x + width as i32, position.y);

        TextMetrics {
            bounding_box: bb,
            next_position: next_pos,
        }
    }

    fn line_height(&self) -> u32 {
        self.font.header().height as u32
    }
}

impl<'a> Font<'a> {
    /// Load a font from a byte slice.
    pub fn new(data: &'a [u8]) -> Result<Self, FontError> {
        Ok(Font { inner: CoreFont::new(data)? })
    }

    /// Access the parsed font header.
    pub fn header(&self) -> &Header {
        &self.inner.header
    }

    /// Access the parsed glyph entries.
    pub fn glyphs(&self) -> &[GlyphEntry] {
        &self.inner.glyphs
    }

    /// Return the pixel format of this font.
    pub fn pixel_format(&self) -> PixelFormat {
        self.inner.header.pixel_format
    }

    /// Return the bounds of a character if it exists in this font.
    pub fn character_bounds(&self, c: char) -> Option<CharacterBounds> {
        self.inner.glyphs.iter().find(|g| g.code == c as u32).map(|g| CharacterBounds {
            width: g.width,
            height: self.inner.header.height,
        })
    }

    /// Return a slice pointing to the raw glyph data for the given entry.
    pub fn glyph_data(&self, entry: &GlyphEntry) -> &[u8] {
        self.inner.glyph_data(entry.code).unwrap_or(&[])
    }

    /// Return glyph data by code point.
    pub fn glyph_data_by_code(&self, code: u32) -> Option<&[u8]> {
        self.inner.glyph_data(code)
    }

    /// Lookup a glyph entry by character.
    pub fn get_glyph_entry(&self, c: char) -> Option<&GlyphEntry> {
        self.inner.glyphs.iter().find(|g| g.code == c as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use font_maker_core::format::{MAGIC, VERSION, HEADER_SIZE, GLYPH_ENTRY_SIZE};

    /// Build a minimal valid AA font with one glyph (code=65 'A', width=5, height=10).
    fn make_test_font_data() -> Vec<u8> {
        let mut buf = Vec::new();
        // Header (14 bytes)
        buf.extend_from_slice(&MAGIC);                    // 0..4
        buf.push(VERSION);                                // 4
        buf.push(PixelFormat::AntiAliased as u8);        // 5
        buf.extend_from_slice(&10u16.to_le_bytes());     // 6..7 height
        buf.extend_from_slice(&0u16.to_le_bytes());      // 8..9 spacing
        buf.extend_from_slice(&1u32.to_le_bytes());      // 10..13 char_count
        // Glyph entry (10 bytes)
        buf.extend_from_slice(&65u32.to_le_bytes());     // code
        buf.extend_from_slice(&5u16.to_le_bytes());      // width
        let data_offset = (HEADER_SIZE + GLYPH_ENTRY_SIZE) as u32;
        buf.extend_from_slice(&data_offset.to_le_bytes()); // data_offset
        // Glyph data: 5x10 = 50 bytes of alpha
        buf.extend_from_slice(&vec![255u8; 50]);
        buf
    }

    #[test]
    fn valid_binary_slice_loads() {
        let data = make_test_font_data();
        let font = Font::new(&data).expect("load font");
        assert_eq!(font.pixel_format(), PixelFormat::AntiAliased);
    }

    #[test]
    fn truncated_slice_returns_error() {
        // Header too short
        let short = &MAGIC[..3];
        assert!(matches!(Font::new(short), Err(FontError::TruncatedFile)));

        // Header present but glyph table truncated
        let mut buf = make_test_font_data();
        buf.truncate(HEADER_SIZE + GLYPH_ENTRY_SIZE - 1);
        assert!(matches!(Font::new(&buf), Err(FontError::TruncatedFile)));
    }

    #[test]
    fn character_bounds_known_code() {
        let data = make_test_font_data();
        let font = Font::new(&data).unwrap();
        let bounds = font.character_bounds('A').expect("A exists");
        assert_eq!(bounds.width, 5);
        assert_eq!(bounds.height, 10);
    }

    #[test]
    fn character_bounds_unknown_code() {
        let data = make_test_font_data();
        let font = Font::new(&data).unwrap();
        assert!(font.character_bounds('Z').is_none());
    }

    #[test]
    fn glyph_data_returns_correct_slice() {
        let data = make_test_font_data();
        let font = Font::new(&data).unwrap();
        let entry = font.get_glyph_entry('A').unwrap();
        let pixel_data = font.glyph_data(entry);
        assert_eq!(pixel_data.len(), 50); // 5x10 AA
        assert!(pixel_data.iter().all(|&v| v == 255));
    }
}

#[cfg(test)]
mod text_renderer_tests {
    use super::*;
    use alloc::vec;
    use embedded_graphics::{
        pixelcolor::Rgb888,
        prelude::*,
        text::{renderer::TextRenderer, Text},
    };
    use embedded_graphics_simulator::SimulatorDisplay;
    use font_maker_core::format::{MAGIC, VERSION, HEADER_SIZE, GLYPH_ENTRY_SIZE};

    fn make_two_char_font() -> Vec<u8> {
        // A (code=65) at x=0..5, B (code=66) at x=7..12 (spacing=2)
        let mut buf = Vec::new();
        // Header
        buf.extend_from_slice(&MAGIC);
        buf.push(VERSION);
        buf.push(PixelFormat::AntiAliased as u8);
        buf.extend_from_slice(&10u16.to_le_bytes()); // height
        buf.extend_from_slice(&2u16.to_le_bytes());  // spacing
        buf.extend_from_slice(&2u32.to_le_bytes());  // char_count
        // Glyph A
        buf.extend_from_slice(&65u32.to_le_bytes());
        buf.extend_from_slice(&5u16.to_le_bytes());
        let off_a = (HEADER_SIZE + 2 * GLYPH_ENTRY_SIZE) as u32;
        buf.extend_from_slice(&off_a.to_le_bytes());
        // Glyph B
        buf.extend_from_slice(&66u32.to_le_bytes());
        buf.extend_from_slice(&5u16.to_le_bytes());
        let off_b = off_a + 50;
        buf.extend_from_slice(&off_b.to_le_bytes());
        // Data: A = solid white, B = half white
        buf.extend_from_slice(&vec![255u8; 50]); // A
        for i in 0..50u8 {
            buf.push(if i % 2 == 0 { 255 } else { 0 }); // B pattern
        }
        buf
    }

    #[test]
    fn text_renderer_draws_single_char() {
        let font_data = make_two_char_font();
        let font = Font::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let mut display = SimulatorDisplay::<Rgb888>::new(Size::new(10, 10));
        let next = style.draw_string("A", Point::new(0, 10), embedded_graphics::text::Baseline::Bottom, &mut display).unwrap();

        // Next position should be after char A: x = 0 + width(5) + spacing(2) = 7
        assert_eq!(next.x, 7);

        // A is solid white, so 5x10 pixels should be white
        let mut white_count = 0;
        for x in 0..5i32 {
            for y in 0..10i32 {
                if display.get_pixel((x, y).into()) == Rgb888::WHITE {
                    white_count += 1;
                }
            }
        }
        assert_eq!(white_count, 50);
    }

    #[test]
    fn text_renderer_draws_multiple_chars_with_spacing() {
        let font_data = make_two_char_font();
        let font = Font::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let mut display = SimulatorDisplay::<Rgb888>::new(Size::new(12, 10));
        let next = style.draw_string("AB", Point::new(0, 10), embedded_graphics::text::Baseline::Bottom, &mut display).unwrap();

        // Next position: A(5+2) + B(5+2) - trailing spacing = 12
        // Actually: A at 0, B at 7, next after B = 7+5+2 = 14
        assert_eq!(next.x, 14);

        // Verify B pattern (every other pixel white)
        let mut white_in_b = 0;
        for x in 7..12i32 {
            for y in 0..10i32 {
                if display.get_pixel((x, y).into()) == Rgb888::WHITE {
                    white_in_b += 1;
                }
            }
        }
        // B has 25 white pixels out of 50 (every other)
        assert_eq!(white_in_b, 25);
    }

    #[test]
    fn text_renderer_measure_string() {
        let font_data = make_two_char_font();
        let font = Font::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let metrics = style.measure_string("AB", Point::new(0, 10), embedded_graphics::text::Baseline::Bottom);

        // Width = A(5) + spacing(2) + B(5) = 12
        assert_eq!(metrics.bounding_box.size.width, 12);
        assert_eq!(metrics.bounding_box.size.height, 10);
    }

    #[test]
    fn text_renderer_line_height() {
        let font_data = make_two_char_font();
        let font = Font::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        assert_eq!(style.line_height(), 10);
    }

    #[test]
    fn text_primitives_integration() {
        // Test using Text primitive directly with our renderer
        let font_data = make_two_char_font();
        let font = Font::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let mut display = SimulatorDisplay::<Rgb888>::new(Size::new(20, 10));
        // Use Baseline::Bottom so text is drawn at y=0..10
        let text = Text::with_baseline("A", Point::new(0, 10), style, Baseline::Bottom);
        text.draw(&mut display).unwrap();

        // A should be white
        for x in 0..5i32 {
            for y in 0..10i32 {
                assert_eq!(display.get_pixel((x, y).into()), Rgb888::WHITE);
            }
        }
    }
}
