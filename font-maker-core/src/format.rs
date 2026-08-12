use crate::error::FontError;

/// Binary font format magic: `"EFM1"`.
pub const MAGIC: [u8; 4] = *b"EFM1";
/// Current format version.
pub const VERSION: u8 = 1;
/// Size of the fixed header in bytes (spacing removed; render-time concern).
pub const HEADER_SIZE: usize = 12;
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

impl TryFrom<u8> for PixelFormat {
    type Error = FontError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Monochrome),
            1 => Ok(Self::AntiAliased),
            _ => Err(FontError::InvalidPixelFormat(v)),
        }
    }
}

/// Binary font header — 12 bytes, little-endian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub version: u8,
    pub pixel_format: PixelFormat,
    pub height: u16,
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
/// Holds the parsed header and a reference to the original byte slice.
/// Glyph entries are parsed lazily from the glyph table (no Vec allocation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font<'a> {
    pub header: Header,
    data: &'a [u8],
}

impl<'a> Font<'a> {
    /// Parse a binary font from a byte slice.
    ///
    /// Validates magic, version, header completeness, per-glyph table bounds,
    /// and that every glyph's data region fits within the file.
    ///
    /// Safe-by-default. For untrusted data (network, user input), use this.
    /// For trusted/embedded fonts where load speed matters, use `new_fast()`.
    ///
    /// Returns `FontError::TruncatedFile` for files shorter than the header.
    /// Returns `FontError::InvalidDataOffset` if any glyph's `data_offset`
    /// points past the end of the file.
    /// Returns `FontError::TruncatedGlyphData` if any glyph's data region
    /// extends past the end of the file.
    pub fn new(data: &'a [u8]) -> Result<Self, FontError> {
        let font = Self::new_fast(data)?;
        font.validate_glyphs()?;
        Ok(font)
    }

    /// Parse a binary font from a byte slice with minimal validation.
    ///
    /// Validates magic, version, header completeness, and per-glyph table bounds.
    /// Does NOT validate individual glyph data regions — caller must trust the data.
    ///
    /// Use this for:
    /// - Embedded fonts compiled into firmware
    /// - Performance-critical hot paths with known-good data
    ///
    /// Returns `FontError::TruncatedFile` for files shorter than the header.
    pub fn new_fast(data: &'a [u8]) -> Result<Self, FontError> {
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
        let pixel_format = PixelFormat::try_from(data[5])?;
        let height = u16::from_le_bytes([data[6], data[7]]);
        let char_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

        let header = Header {
            magic: MAGIC,
            version,
            pixel_format,
            height,
            char_count,
        };

        // 5. Per-glyph table must fit after the header.
        let table_end = HEADER_SIZE
            .checked_add(GLYPH_ENTRY_SIZE * char_count as usize)
            .ok_or(FontError::TruncatedFile)?;
        if data.len() < table_end {
            return Err(FontError::TruncatedFile);
        }

        Ok(Font { header, data })
    }

    /// Validate per-glyph data regions (data_offset + data_size within file bounds).
    ///
    /// Called internally by `new()`; can also be called explicitly on a font
    /// created with `new_fast()` if the caller later wants to verify integrity.
    pub fn validate_glyphs(&self) -> Result<(), FontError> {
        let char_count = self.header.char_count;
        let pixel_format = self.header.pixel_format;
        let height = self.header.height;
        let data_len = self.data.len();

        for i in 0..char_count {
            let base = HEADER_SIZE + (i as usize) * GLYPH_ENTRY_SIZE;
            let width = u16::from_le_bytes([self.data[base + 4], self.data[base + 5]]);
            let data_offset = u32::from_le_bytes([
                self.data[base + 6],
                self.data[base + 7],
                self.data[base + 8],
                self.data[base + 9],
            ]);

            let offset = data_offset as usize;
            if offset > data_len {
                return Err(FontError::InvalidDataOffset {
                    offset: data_offset,
                    file_size: data_len,
                });
            }

            let data_size = glyph_data_size(width, height, pixel_format);
            if offset + data_size > data_len {
                return Err(FontError::TruncatedGlyphData {
                    offset: data_offset,
                    required: data_size,
                });
            }
        }

        Ok(())
    }

    /// Return the glyph entry for the given code point (linear scan of glyph table).
    ///
    /// Returns `None` if the character is not in the font.
    ///
    /// Optimized: reads only the 4-byte code field during the scan;
    /// deserializes the full GlyphEntry only on match.
    pub fn get_glyph_entry(&self, code: u32) -> Option<GlyphEntry> {
        let char_count = self.header.char_count as usize;
        for i in 0..char_count {
            let base = HEADER_SIZE + i * GLYPH_ENTRY_SIZE;
            // Read only the code field (first 4 bytes) for the scan.
            let entry_code = u32::from_le_bytes([
                self.data[base],
                self.data[base + 1],
                self.data[base + 2],
                self.data[base + 3],
            ]);
            if entry_code == code {
                // Match found — deserialize the full entry once.
                let width = u16::from_le_bytes([
                    self.data[base + 4],
                    self.data[base + 5],
                ]);
                let data_offset = u32::from_le_bytes([
                    self.data[base + 6],
                    self.data[base + 7],
                    self.data[base + 8],
                    self.data[base + 9],
                ]);
                return Some(GlyphEntry {
                    code,
                    width,
                    data_offset,
                });
            }
        }
        None
    }

    /// Return a slice pointing to the raw glyph data for the given entry.
    pub fn glyph_data(&self, entry: &GlyphEntry) -> &[u8] {
        let off = entry.data_offset as usize;
        let len = glyph_data_size(entry.width, self.header.height, self.header.pixel_format);
        &self.data[off..off + len]
    }
}

/// Calculate the byte size of glyph data given dimensions and pixel format.
///
/// # Arguments
/// * `width` - glyph pixel width
/// * `height` - glyph pixel height
/// * `pixel_format` - encoding format (AntiAliased = 1 byte/px, Monochrome = 1 bit/px)
///
/// # Returns
/// Byte count required to store the glyph.
pub fn glyph_data_size(width: u16, height: u16, pixel_format: PixelFormat) -> usize {
    let px_count = width as usize * height as usize;
    match pixel_format {
        PixelFormat::AntiAliased => px_count,
        PixelFormat::Monochrome => (px_count + 7) / 8,
    }
}
