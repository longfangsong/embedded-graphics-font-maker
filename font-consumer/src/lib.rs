#![no_std]
extern crate alloc;

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
use font_maker_core::format::{Font as CoreFont, GlyphEntry, PixelFormat};
use font_maker_core::error::FontError;

/// Zero-alloc iterator over pixels of a single glyph.
struct GlyphPixelIter<'a, C> {
    data: &'a [u8],
    width: usize,
    height: usize,
    pos: Point,
    fg8: Rgb888,
    bg8: Rgb888,
    fmt: PixelFormat,
    idx: usize,
    _marker: core::marker::PhantomData<C>,
}

impl<'a, C: PixelColor + From<Rgb888> + Into<Rgb888>> Iterator for GlyphPixelIter<'a, C> {
    type Item = Pixel<C>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.fmt {
            PixelFormat::AntiAliased => {
                while self.idx < self.data.len() {
                    let alpha = self.data[self.idx];
                    let px = (self.idx % self.width) as i32;
                    let py = (self.idx / self.width) as i32;
                    self.idx += 1;
                    if alpha == 0 {
                        continue;
                    }
                    let blended8 = Rgb888::new(
                        blend_channel(self.bg8.r(), self.fg8.r(), alpha),
                        blend_channel(self.bg8.g(), self.fg8.g(), alpha),
                        blend_channel(self.bg8.b(), self.fg8.b(), alpha),
                    );
                    return Some(Pixel(
                        Point::new(self.pos.x + px, self.pos.y + py),
                        C::from(blended8),
                    ));
                }
            }
            PixelFormat::Monochrome => {
                while self.idx < self.width * self.height {
                    let bit_idx = self.idx;
                    let byte_idx = bit_idx / 8;
                    let bit = 7 - (bit_idx % 8);
                    self.idx += 1;
                    if byte_idx >= self.data.len() {
                        break;
                    }
                    let fg = (self.data[byte_idx] >> bit) & 1 != 0;
                    if fg {
                        let px = (bit_idx % self.width) as i32;
                        let py = (bit_idx / self.width) as i32;
                        return Some(Pixel(
                            Point::new(self.pos.x + px, self.pos.y + py),
                            C::from(self.fg8),
                        ));
                    }
                }
            }
        }
        None
    }
}

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
    pub font: &'a CoreFont<'a>,
    /// Text color.
    pub text_color: C,
    /// Background color (optional).
    pub background_color: Option<C>,
    /// Character spacing. Defaults to 2 if None.
    pub char_spacing: Option<u32>,
}

impl<'a, C: PixelColor + From<Rgb888> + Into<Rgb888>> FontTextStyle<'a, C> {
    /// Create a new text style with the given font and text color.
    pub fn new(font: &'a CoreFont<'a>, text_color: C) -> Self {
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
        let glyph = self.font.get_glyph_entry(c as u32).expect("glyph not found");
        let data = self.font.glyph_data(&glyph);
        let width = glyph.width as usize;
        let height = self.font.header.height as usize;
        let fg8: Rgb888 = self.text_color.into();
        let bg8: Rgb888 = self.background_color.map(|c| c.into()).unwrap_or(Rgb888::BLACK);

        target.draw_iter(GlyphPixelIter {
            data,
            width,
            height,
            pos,
            fg8,
            bg8,
            fmt: self.font.header.pixel_format,
            idx: 0,
            _marker: core::marker::PhantomData,
        })?;

        Ok(glyph.width as u32)
    }
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
        let height = self.font.header.height as i32;
        let y_offset = match baseline {
            Baseline::Top => 0,
            Baseline::Alphabetic => 0,
            Baseline::Middle => -(height / 2),
            Baseline::Bottom => -height,
        };
        let mut x = position.x;
        let y = position.y + y_offset;
        let spacing = self.char_spacing.unwrap_or(2) as i32;

        for c in text.chars() {
            if let Some(entry) = self.font.get_glyph_entry(c as u32) {
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
        let spacing = self.char_spacing.unwrap_or(2);

        for (i, c) in text.chars().enumerate() {
            if let Some(entry) = self.font.get_glyph_entry(c as u32) {
                width += entry.width as u32;
                if i < text.chars().count() - 1 {
                    width += spacing;
                }
            }
        }

        let height = self.font.header.height as u32;
        let bb = Rectangle::new(position, Size::new(width, height));
        let next_pos = Point::new(position.x + width as i32, position.y);

        TextMetrics {
            bounding_box: bb,
            next_position: next_pos,
        }
    }

    fn line_height(&self) -> u32 {
        self.font.header.height as u32
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use font_maker_core::format::{MAGIC, VERSION, HEADER_SIZE, GLYPH_ENTRY_SIZE};

    /// Build a minimal valid AA font with one glyph (code=65 'A', width=5, height=10).
    fn make_test_font_data() -> Vec<u8> {
        let mut buf = Vec::new();
        // Header (12 bytes)
        buf.extend_from_slice(&MAGIC);                    // 0..4
        buf.push(VERSION);                                // 4
        buf.push(PixelFormat::AntiAliased as u8);        // 5
        buf.extend_from_slice(&10u16.to_le_bytes());     // 6..7 height
        buf.extend_from_slice(&1u32.to_le_bytes());      // 8..11 char_count
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
        let font = CoreFont::new(&data).expect("load font");
        assert_eq!(font.header.pixel_format, PixelFormat::AntiAliased);
    }

    #[test]
    fn truncated_slice_returns_error() {
        // Header too short
        let short = &MAGIC[..3];
        assert!(matches!(CoreFont::new(short), Err(FontError::TruncatedFile)));

        // Header present but glyph table truncated
        let mut buf = make_test_font_data();
        buf.truncate(HEADER_SIZE + GLYPH_ENTRY_SIZE - 1);
        assert!(matches!(CoreFont::new(&buf), Err(FontError::TruncatedFile)));
    }

    #[test]
    fn character_bounds_known_code() {
        let data = make_test_font_data();
        let font = CoreFont::new(&data).unwrap();
        let entry = font.get_glyph_entry(0x41).expect("A exists");
        assert_eq!(entry.width, 5);
        assert_eq!(font.header.height, 10);
    }

    #[test]
    fn character_bounds_unknown_code() {
        let data = make_test_font_data();
        let font = CoreFont::new(&data).unwrap();
        assert!(font.get_glyph_entry(0x5A).is_none());
    }

    #[test]
    fn glyph_data_returns_correct_slice() {
        let data = make_test_font_data();
        let font = CoreFont::new(&data).unwrap();
        let entry = font.get_glyph_entry(0x41).unwrap();
        let pixel_data = font.glyph_data(&entry);
        assert_eq!(pixel_data.len(), 50); // 5x10 AA
        assert!(pixel_data.iter().all(|&v| v == 255));
    }
}

#[cfg(test)]
mod text_renderer_tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use embedded_graphics::{
        pixelcolor::Rgb888,
        prelude::*,
        text::{renderer::TextRenderer, Text},
    };
    use embedded_graphics_simulator::SimulatorDisplay;
    use font_maker_core::format::{MAGIC, VERSION, HEADER_SIZE, GLYPH_ENTRY_SIZE};

    fn make_two_char_font() -> Vec<u8> {
        // A (code=65) at x=0..5, B (code=66) at x=7..12
        let mut buf = Vec::new();
        // Header (12 bytes)
        buf.extend_from_slice(&MAGIC);
        buf.push(VERSION);
        buf.push(PixelFormat::AntiAliased as u8);
        buf.extend_from_slice(&10u16.to_le_bytes()); // height
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
        let font = CoreFont::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let mut display = SimulatorDisplay::<Rgb888>::new(Size::new(10, 10));
        let next = style.draw_string("A", Point::new(0, 10), embedded_graphics::text::Baseline::Bottom, &mut display).unwrap();

        // Next position: x = 0 + width(5) + default_spacing(2) = 7
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
        let font = CoreFont::new(&font_data).unwrap();
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
        let font = CoreFont::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        let metrics = style.measure_string("AB", Point::new(0, 10), embedded_graphics::text::Baseline::Bottom);

        // Width = A(5) + spacing(2) + B(5) = 12
        assert_eq!(metrics.bounding_box.size.width, 12);
        assert_eq!(metrics.bounding_box.size.height, 10);
    }

    #[test]
    fn text_renderer_line_height() {
        let font_data = make_two_char_font();
        let font = CoreFont::new(&font_data).unwrap();
        let style = FontTextStyle::new(&font, Rgb888::WHITE);

        assert_eq!(style.line_height(), 10);
    }

    #[test]
    fn text_primitives_integration() {
        // Test using Text primitive directly with our renderer
        let font_data = make_two_char_font();
        let font = CoreFont::new(&font_data).unwrap();
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
