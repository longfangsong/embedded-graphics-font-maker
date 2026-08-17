# 0001 — Binary Font Format and Toolchain

## Problem Statement

嵌入式开发中需要在屏幕上渲染文字，但嵌入式图形库（如 `embedded_graphics`）自带的字体是编译时硬编码的位图，无法灵活更换。开发者希望从自定义的 PNG 字符图集生成可在嵌入式设备上使用的二进制字体文件，并提供一套工具链完成「PNG → 二进制 → 渲染」的完整流程。

## Solution

构建一个 Rust workspace，包含三个 crate：

- **`font-maker-core`** — 二进制字体格式的解析/序列化库，`no_std` + WASM 兼容
- **`font-maker-cli`** — CLI 工具，从 PNG 生成二进制字体文件，核心逻辑依赖 `font-maker-core`，可编译为 WASM 供前端调用
- **`font-consumer`** — 为 `embedded_graphics` 提供字体消费能力，实现 `TextRenderer` trait

完整流程：开发者准备一张 PNG 字符图集 → 用 CLI 转换为二进制字体 → 在嵌入式项目中通过 `font-consumer` 加载二进制数据并渲染文本。

## User Stories

1. As an embedded developer, I want to convert a PNG character atlas into a compact binary font file, so that I can use custom fonts on my device.
2. As an embedded developer, I want the binary font to support anti-aliased (8bpp alpha) rendering, so that text looks smooth on my screen.
3. As an embedded developer, I want the binary font to optionally support monochrome (1bpp) rendering, so that I can save flash space when AA is not needed.
4. As an embedded developer, I want the CLI to automatically detect character boundaries from a single-row PNG, so that I don't need to manually configure each character's position.
5. As an embedded developer, I want the auto-detected character codes to be configurable (starting ASCII code), so that I can map characters to whatever codes I need.
6. As an embedded developer, I want character spacing to be configurable at render time, so that the same font can be reused with different layout requirements.
7. As an embedded developer, I want to override character spacing in the renderer, so that I can fine-tune layout for my specific display.
8. As an embedded developer, I want to load a binary font from a `&[u8]` byte slice, so that I can use it whether the data comes from `include_bytes!` (flash), file system, or network.
9. As an embedded developer, I want the consumer crate to implement `embedded_graphics::text::renderer::TextRenderer`, so that I can use it with embedded_graphics's text rendering infrastructure.
10. As an embedded developer, I want AA glyphs to be rendered with src-over alpha blending, so that text blends smoothly with arbitrary backgrounds.
11. As a frontend developer, I want the font conversion logic compiled to WASM, so that I can build a web-based PNG-to-font tool.
12. As an embedded developer, I want the binary font format to be self-describing (magic + version in header), so that I can detect incompatible formats at load time.
13. As an embedded developer, I want the per-glyph data to store only the actual character pixels (excluding inter-character spacing), so that the binary font is as compact as possible.
14. As an embedded developer, I want the consumer to handle both 8bpp and 1bpp formats efficiently, so that I can choose the best trade-off between quality and size per font.
15. As an embedded developer, I want the `font-maker-core` crate to be `no_std` compatible, so that it can run on microcontrollers and in WASM without std.
16. As a developer, I want the CLI to output a single binary file containing the complete font, so that deployment is simple (one file to include).

## Implementation Decisions

### Binary Font Format v1

**Header** (12 bytes total):

| Field | Type | Description |
|-------|------|-------------|
| magic | `[u8; 4]` | `"EFM1"` — identifies the format |
| version | `u8` | `1` — format version |
| pixel_format | `u8` | `0` = monochrome (1bpp), `1` = anti-aliased (8bpp) |
| height | `u16` | Uniform glyph height (all glyphs share this) |
| char_count | `u32` | Number of glyphs |

**Per-glyph table** (10 bytes each, stored after header):

| Field | Type | Description |
|-------|------|-------------|
| code | `u32` | Unicode code point |
| width | `u16` | Actual glyph pixel width (content only, excludes spacing) |
| data_offset | `u32` | Byte offset from start of file to this glyph's data |

**Glyph data** (stored sequentially after the table):

- 8bpp (AA): `width × height` bytes, one alpha value per pixel (0 = transparent, 255 = opaque)
- 1bpp (mono): `(width × height + 7) / 8` bytes, 8 pixels packed per byte (1 = foreground, 0 = background)

**Layout**: Header → Per-glyph table (char_count entries) → Glyph data (sequential, in same order as table).

### Binary Font Format v2

v2 appends one field to the header. Everything after the header — the per-glyph
table and the glyph data — is unchanged.

**Header** (14 bytes total):

| Field | Type | Description |
|-------|------|-------------|
| magic | `[u8; 4]` | `"EFM1"` — identifies the format |
| version | `u8` | `2` — format version |
| pixel_format | `u8` | `0` = monochrome (1bpp), `1` = anti-aliased (8bpp) |
| height | `u16` | Uniform glyph height (all glyphs share this) |
| char_count | `u32` | Number of glyphs |
| baseline | `u16` | Rows from the top of the glyph box down to the alphabetic baseline; `<= height` |

- The baseline is font-level metadata, not per-glyph: every glyph shares the
  same box, so they share the same baseline row.
- It is fixed at generation time. The web tool knows it exactly — it is the
  canvas `fillText` y used with `textBaseline = 'alphabetic'`. The CLI takes it
  via `--baseline`, defaulting to `height`, i.e. assuming the PNG atlas was
  cropped so that the baseline is the last row.

### Glyph Box (web tool)

The glyph box is derived from the font, not from the requested pixel size:

1. Render on a canvas sized from the font's own metrics —
   `ceil(fontBoundingBoxAscent) + ceil(fontBoundingBoxDescent)` plus 1px of
   padding on each side for anti-aliasing spill. Sizing it from the nominal
   pixel size instead would clip: PingFang declares a 1.06em ascent, so at 18px
   its tall punctuation (`( ) [ | ~`) overflows an 18-row box by 2 rows.
2. Scan the rendered atlas for the rows that contain ink, across *all* selected
   characters — every glyph shares one box, so the box is their union.
3. Crop to those rows: `height = bottom - top + 1`, `baseline = baselineY - top`.

So `height` is a measured output, not the pixel size the user typed, and two
fonts cut from the same TTF at the same size can differ in height if their
character sets differ. Blank rows are never stored: a digits-only PingFang at
48px comes out 37 rows instead of 48 (−21% of the file) with an identical
rendering.
- Readers still accept v1 files: those have a 12-byte header and no baseline,
  so the parser reports `baseline == height` for them, matching the CLI
  default. Writers always emit v2.

### Pixel Format

- The `pixel_format` field in the header selects the encoding.
- Consumer renders differently based on format: 8bpp does direct alpha blending; 1bpp extracts bits on-the-fly (roughly 4-6× slower per pixel, but 8× smaller).
- The consumer does not need to decompress 1bpp into a full buffer — it extracts bits during the blending loop.

### Character Spacing

- Spacing is a render-time concern, not stored in the font. The font describes only glyph pixels.
- The consumer (renderer) specifies spacing as a layout parameter. A default of 2 is used if unspecified.
- This keeps the font reusable across different layout requirements without re-conversion.

### PNG Auto-Detection

- Only single-row PNGs are supported.
- A column is considered "content" if any pixel in that column has `alpha > 0`.
- Contiguous content columns form a character region.
- Regions are sorted by x position and assigned sequential ASCII codes starting from a configurable base (default `0x20`).
- `advance_width` is auto-calculated as `max(width of all detected characters)`, overridable via CLI flag.

### Consumer API

- `Font` struct holds parsed header, glyph table, and a reference to the raw byte slice.
- `Font::new(slice: &[u8]) → Result<Self, Error>` — parses and validates the binary data.
- Implements `embedded_graphics::text::renderer::TextRenderer`.
- Does not assume how the byte data was obtained (no file IO, no `include_bytes!` — that's the caller's choice).

### Crate Structure

```
embedded-graphics-font-maker/
├── Cargo.toml              (workspace)
├── font-maker-core/        (lib, no_std, wasm-target compatible)
│   └── src/lib.rs
├── font-maker-cli/         (binary, depends on font-maker-core)
│   └── src/main.rs
└── font-consumer/          (lib, depends on font-maker-core + embedded_graphics)
    └── src/lib.rs
```

- `font-maker-core` contains: format types, serialization, deserialization, error types. No dependencies on `std` or external crates (except optionally `png` for the CLI).
- `font-maker-cli` contains: CLI entry point (clap), PNG parsing (image/png crate), conversion pipeline. Compilable to `wasm32-unknown-unknown` via `wasm-pack`.
- `font-consumer` contains: `Font` type, `TextRenderer` implementation, alpha blending logic. Depends on `font-maker-core` and `embedded_graphics`.

### TextRenderer Implementation

The `TextRenderer` trait requires (based on embedded_graphics conventions):

- `character_bounds(&self, c: char) → Option<Rectangle>` — returns the cell size (`advance_width × height`) for a known character, `None` if unknown.
- `draw_char(&self, c: char, location: Point, style: &mut dyn DrawTarget) → Result<(), Self::Error>` — renders one glyph at the given location with src-over alpha blending.

For AA rendering, the draw loop iterates over each pixel in the glyph's `width × height` data, computes the source color from the alpha value, and blends it with the existing pixel via src-over compositing.

For 1bpp rendering, the same loop extracts one bit per pixel on-the-fly before blending.

## Testing Decisions

### Seam 1: Binary Format Parser (font-maker-core) — Unit Tests

The format parser is a pure function: `&[u8] → Font`. This is the highest and cleanest test seam — no I/O, no mocks.

- **What to test**:
  - Valid header parsing (magic, version, all fields)
  - Invalid magic → error
  - Invalid version → error
  - Per-glyph table parsing (correct offsets, codes, widths)
  - Glyph data boundary checks (data_offset within file bounds)
  - Both 8bpp and 1bpp data size calculations
  - Empty font (char_count = 0)
  - Truncated file → error (not panic)

- **How**: Construct byte vectors manually (using `vec![...]` or `bytes` crate) and pass to `Font::new()`. Assert on returned `Result`.

### Seam 2: PNG Auto-Detection (font-maker-cli) — Integration Tests

- **What to test**:
  - Single-row PNG with known character positions → correct bounding boxes
  - PNG with varying character widths → correct max-width advance calculation
  - PNG with anti-aliased edges (partial alpha) → characters still detected
  - Empty PNG (no content) → empty font or error

- **How**: Create fixture PNG files (using Python/Pillow in CI or checked-in binaries), run CLI, compare output binary against expected.

### Seam 3: Font Loading from &[u8] (font-consumer) — Unit Tests

- **What to test**:
  - `Font::new()` with a valid binary slice → succeeds
  - `Font::new()` with truncated data → fails
  - `character_bounds()` returns correct dimensions for known characters
  - `character_bounds()` returns `None` for unknown characters

- **How**: Embed a small valid binary font as a `&[u8]` constant in the test, exercise the API.

### Not Tested (for now)

- Actual pixel-perfect rendering output (requires mock DrawTarget, can be added later)
- WASM compilation (verified by CI when WASM target is added)

## Out of Scope

- Multi-row PNG support (grid layouts)
- Manual character-to-code configuration (auto-detect only, for now)
- Kerning / variable advance per character (uniform advance only)
- Color fonts (grayscale alpha only, no per-pixel color)
- Font subsetting (include all detected characters)
- Build-script (`build.rs`) integration
- JavaScript frontend (WASM is exported, wrapping is future work)
- Font editing or merging tools

## Further Notes

- The PNG format is treated as a character atlas: characters are extracted by their bounding boxes, and inter-character spacing in the PNG is ignored. The advance width is a uniform font-level configuration.
- The binary format is intentionally minimal (v1): magic, version, pixel_format, height, advance_width, char_count, and per-glyph (code, width, data_offset). Metadata like font name, baseline, ascent/descent can be added in future versions.
- `no_std` in `font-maker-core` means no `std`, no `alloc` by default. The parser works on `&[u8]` slices without allocation. The `Font` struct holds references to the input slice.
- The consumer's `TextRenderer` implementation is the single integration point with `embedded_graphics`. All rendering logic (alpha blending, bit extraction for 1bpp) lives in this crate.
