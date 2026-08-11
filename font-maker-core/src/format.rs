use alloc::vec::Vec;

use crate::error::FontError;

/// Binary font format magic: `"EFM1"`.
pub const MAGIC: [u8; 4] = *b"EFM1";
/// Current format version.
pub const VERSION: u8 = 1;
/// Size of the fixed header in bytes.
pub const HEADER_SIZE: usize = 14;
/// Size of each per-glyph table entry in bytes.
pub const GLYPH_ENTRY_SIZE: usize = 10;

/// Pixel format stored in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelFormat {
    /// Monochrome: 1 bit per pixel, 8 pixels packed per byte.
    Monochrome = 0,
    /// Anti-aliased: 8 bits per pixel (alpha value).
    AntiAliased = 1,
}

impl PixelFormat {
    /// Decode a raw `u8` from the file into a `PixelFormat`, or `None` if unknown.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Monochrome),
            1 => Some(Self::AntiAliased),
            _ => None,
        }
    }
}

/// Binary font header — 14 bytes, little-endian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub version: u8,
    pub pixel_format: PixelFormat,
    pub height: u16,
    pub spacing: u16,
    pub char_count: u32,
}

/// One entry in the per-glyph table — 10 bytes, little-endian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphEntry {
    /// Unicode code point.
    pub code: u32,
    /// Actual glyph pixel width (content only, excludes spacing).
    pub width: u16,
    /// Byte offset from start of file to this glyph's data region.
    pub data_offset: u32,
}

/// A parsed binary font.
///
/// Holds the parsed header, the per-glyph table, and a reference to the
/// original byte slice so glyph data can be accessed without copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font<'a> {
    pub header: Header,
    pub glyphs: Vec<GlyphEntry>,
    data: &'a [u8],
}

impl<'a> Font<'a> {
    /// Parse a binary font from a byte slice.
    ///
    /// Validates magic, version, header completeness, per-glyph table bounds,
    /// and that every glyph's data region fits within the file.
    ///
    /// Returns `FontError::TruncatedFile` for files shorter than the header.
    /// Returns `FontError::InvalidDataOffset` if any glyph's `data_offset`
    /// points past the end of the file.
    /// Returns `FontError::TruncatedGlyphData` if any glyph's data region
    /// extends past the end of the file.
    pub fn new(data: &'a [u8]) -> Result<Self, FontError> {
        // 1. Header must fit.
        if data.len() < HEADER_SIZE {
            return Err(FontError::TruncatedFile);
        }

        // 2. Magic.
        if data[0..4] != MAGIC {
            return Err(FontError::InvalidMagic);
        }

        // 3. Version.
        let version = data[4];
        if version != VERSION {
            return Err(FontError::UnsupportedVersion(version));
        }

        // 4. Decode header fields.
        let pixel_format = PixelFormat::from_u8(data[5])
            .ok_or(FontError::UnsupportedVersion(data[5]))?;
        let height = u16::from_le_bytes([data[6], data[7]]);
        let spacing = u16::from_le_bytes([data[8], data[9]]);
        let char_count = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);

        let header = Header {
            magic: MAGIC,
            version,
            pixel_format,
            height,
            spacing,
            char_count,
        };

        // 5. Empty font — nothing more to validate.
        if char_count == 0 {
            return Ok(Font {
                header,
                glyphs: Vec::new(),
                data,
            });
        }

        // 6. Per-glyph table must fit after the header.
        let table_end = HEADER_SIZE
            .checked_add(GLYPH_ENTRY_SIZE * char_count as usize)
            .ok_or(FontError::TruncatedFile)?;
        if data.len() < table_end {
            return Err(FontError::TruncatedFile);
        }

        // 7. Parse each glyph entry and validate data regions.
        let mut glyphs = Vec::with_capacity(char_count as usize);
        for i in 0..char_count {
            let base = HEADER_SIZE + (i as usize) * GLYPH_ENTRY_SIZE;
            let code = u32::from_le_bytes([
                data[base],
                data[base + 1],
                data[base + 2],
                data[base + 3],
            ]);
            let width = u16::from_le_bytes([data[base + 4], data[base + 5]]);
            let data_offset = u32::from_le_bytes([
                data[base + 6],
                data[base + 7],
                data[base + 8],
                data[base + 9],
            ]);

            // data_offset must point within the file.
            let offset = data_offset as usize;
            if offset > data.len() {
                return Err(FontError::InvalidDataOffset {
                    offset: data_offset,
                    file_size: data.len(),
                });
            }

            // Data size for this glyph.
            let px_count = width as usize * height as usize;
            let data_size = match pixel_format {
                PixelFormat::AntiAliased => px_count,
                PixelFormat::Monochrome => (px_count + 7) / 8,
            };

            // Glyph data must fit within the file.
            if offset + data_size > data.len() {
                return Err(FontError::TruncatedGlyphData {
                    offset: data_offset,
                    required: data_size,
                });
            }

            glyphs.push(GlyphEntry {
                code,
                width,
                data_offset,
            });
        }

        Ok(Font {
            header,
            glyphs,
            data,
        })
    }

    /// Return a slice pointing to the raw glyph data for the given code point.
    ///
    /// Returns `None` if the character is not in the font.
    pub fn glyph_data(&self, code: u32) -> Option<&[u8]> {
        self.glyphs
            .iter()
            .find(|g| g.code == code)
            .map(|g| {
                let off = g.data_offset as usize;
                let px_count = g.width as usize * self.header.height as usize;
                let len = match self.header.pixel_format {
                    PixelFormat::AntiAliased => px_count,
                    PixelFormat::Monochrome => (px_count + 7) / 8,
                };
                &self.data[off..off + len]
            })
    }

    /// Serialize this font back to a byte vector.
    ///
    /// Layout: Header (14 bytes) → Per-glyph table (char_count × 10 bytes) →
    /// sequential glyph data.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.serialized_size());

        // Header.
        buf.extend_from_slice(&MAGIC);
        buf.push(self.header.version);
        buf.push(self.header.pixel_format as u8);
        buf.extend_from_slice(&self.header.height.to_le_bytes());
        buf.extend_from_slice(&self.header.spacing.to_le_bytes());
        buf.extend_from_slice(&self.header.char_count.to_le_bytes());

        // Per-glyph table.
        let mut data_cursor = HEADER_SIZE + GLYPH_ENTRY_SIZE * self.glyphs.len();
        for entry in &self.glyphs {
            buf.extend_from_slice(&entry.code.to_le_bytes());
            buf.extend_from_slice(&entry.width.to_le_bytes());
            buf.extend_from_slice(&(data_cursor as u32).to_le_bytes());
            let px_count = entry.width as usize * self.header.height as usize;
            let data_len = match self.header.pixel_format {
                PixelFormat::AntiAliased => px_count,
                PixelFormat::Monochrome => (px_count + 7) / 8,
            };
            data_cursor += data_len;
        }

        // Glyph data — pull from the original bytes.
        for entry in &self.glyphs {
            let off = entry.data_offset as usize;
            let px_count = entry.width as usize * self.header.height as usize;
            let len = match self.header.pixel_format {
                PixelFormat::AntiAliased => px_count,
                PixelFormat::Monochrome => (px_count + 7) / 8,
            };
            buf.extend_from_slice(&self.data[off..off + len]);
        }

        buf
    }

    fn serialized_size(&self) -> usize {
        let table_size = GLYPH_ENTRY_SIZE * self.glyphs.len();
        let data_size: usize = self.glyphs.iter().map(|g| glyph_data_size(g, &self.header)).sum();
        HEADER_SIZE + table_size + data_size
    }
}

fn glyph_data_size(g: &GlyphEntry, header: &Header) -> usize {
    let px_count = g.width as usize * header.height as usize;
    match header.pixel_format {
        PixelFormat::AntiAliased => px_count,
        PixelFormat::Monochrome => (px_count + 7) / 8,
    }
}


