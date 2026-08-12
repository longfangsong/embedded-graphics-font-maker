# Backlog

### [TODO] render.rs: unknown character handling

**Problem:** Unknown characters (no glyph entry) are silently skipped with zero width advance. If two unknown chars appear consecutively, no visual feedback; if mixed with known chars, layout is "correct" but user can't tell a char was dropped.

**State:** Deferred. Current behavior is "skip silently" — acceptable for now.

**Options:**
- (i) Keep skip-silently (current)
- (ii) Unknown char takes `font.height` width as placeholder
- (iii) Unknown char draws a box (□)

**Recommendation:** (i) for now, revisit if users report confusion.
