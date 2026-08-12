use core::fmt;

/// Errors that can occur when parsing a binary font file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// File is empty or shorter than the 20-byte header.
    TruncatedFile,
    /// Magic bytes are not `"EFM1"`.
    InvalidMagic,
    /// Version byte is not 1.
    UnsupportedVersion(u8),
    /// Byte at the pixel_format position is not a valid format code.
    InvalidPixelFormat(u8),
    /// A per-glyph data_offset points outside the file bounds.
    InvalidDataOffset { offset: u32, file_size: usize },
    /// Glyph data extends beyond the end of the file.
    TruncatedGlyphData { offset: u32, required: usize },
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::TruncatedFile => write!(f, "file too short for header"),
            FontError::InvalidMagic => write!(f, "invalid magic bytes"),
            FontError::UnsupportedVersion(v) => write!(f, "unsupported version: {v}"),
            FontError::InvalidPixelFormat(v) => write!(f, "invalid pixel format byte: {v}"),
            FontError::InvalidDataOffset { offset, file_size } => {
                write!(f, "data_offset {offset} exceeds file size {file_size}")
            }
            FontError::TruncatedGlyphData { offset, required } => {
                write!(
                    f,
                    "glyph data at offset {offset} requires {required} bytes but file is truncated"
                )
            }
        }
    }
}
