//! ASCII atlas derived from Spleen 5x8 2.2.0.
//!
//! Source: https://github.com/fcambus/spleen/blob/master/spleen-5x8.bdf
//! Copyright (c) 2018-2026, Frederic Cambus. BSD-2-Clause.
//!
//! Glyph resolution is per-character and fail-closed: printable ASCII is
//! resolved from the pinned 2.2.0 atlas, the newline marker has a dedicated
//! arrow glyph, and every other Unicode code point (including CJK/wide text)
//! is deterministically substituted with a hollow-box placeholder so the
//! lossy substitution is visually explicit and can never be confused with a
//! real atlas glyph such as `?`. The substitution is part of the rendered
//! pixels, so page digests change whenever fallback coverage changes.

pub const SPLEEN_5X8_SOURCE_REVISION: &str = "spleen-5x8-2.2.0-ascii";
pub const SPLEEN_5X8_AA_REVISION: &str = "dtoken-spleen-5x8-aa-kernel-v1";
/// Revision of the per-glyph fail-closed Unicode/CJK fallback kernel. The
/// fallback glyph is d-token-owned (not copied from pxpipe or any other
/// runtime); it is deliberately distinct from every atlas glyph.
pub const SPLEEN_5X8_FALLBACK_REVISION: &str = "dtoken-spleen-5x8-unicode-fallback-v1";

/// Hollow-box placeholder for Unicode/CJK code points outside the ASCII
/// atlas. A border-only 5x8 box: unsupported text remains visible as a
/// clearly marked cell rather than being silently dropped or mis-mapped.
#[rustfmt::skip]
pub const SPLEEN_5X8_UNICODE_FALLBACK_GLYPH: [u8; 8] = [
    0xF8, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0xF8,
];

/// Whether `character` has an exact atlas glyph (printable ASCII or the
/// newline marker). Every other code point takes the fallback glyph.
pub fn is_supported(character: char) -> bool {
    (' '..='~').contains(&character) || character == '↵'
}

#[rustfmt::skip]
pub const SPLEEN_5X8_ASCII: [[u8; 8]; 95] = [
    [0,0,0,0,0,0,0,0], [32,32,32,32,32,0,32,0], [80,80,80,0,0,0,0,0],
    [0,80,248,80,80,248,80,0], [32,112,160,96,48,48,224,32], [16,144,160,32,64,80,144,128],
    [32,80,80,96,168,144,104,0], [32,32,32,0,0,0,0,0], [16,32,64,64,64,64,32,16],
    [64,32,16,16,16,16,32,64], [0,0,144,96,240,96,144,0], [0,0,32,32,248,32,32,0],
    [0,0,0,0,0,32,32,64], [0,0,0,0,240,0,0,0], [0,0,0,0,0,0,32,0],
    [16,16,32,32,64,64,128,128], [0,96,144,176,208,144,96,0], [0,32,96,32,32,32,112,0],
    [0,96,144,16,96,128,240,0], [0,96,144,32,16,144,96,0], [0,128,160,160,240,32,32,0],
    [0,240,128,224,16,16,224,0], [0,96,128,224,144,144,96,0], [0,240,144,16,32,64,64,0],
    [0,96,144,96,144,144,96,0], [0,96,144,144,112,16,96,0], [0,0,0,32,0,0,32,0],
    [0,0,0,32,0,32,32,64], [0,16,32,64,64,32,16,0], [0,0,0,240,0,240,0,0],
    [0,64,32,16,16,32,64,0], [96,144,16,32,64,0,64,0], [0,96,144,176,176,128,112,0],
    [0,96,144,144,240,144,144,0], [0,224,144,224,144,144,224,0], [0,112,128,128,128,128,112,0],
    [0,224,144,144,144,144,224,0], [0,112,128,224,128,128,112,0], [0,112,128,128,224,128,128,0],
    [0,112,128,176,144,144,112,0], [0,144,144,240,144,144,144,0], [0,112,32,32,32,32,112,0],
    [0,112,32,32,32,32,192,0], [0,144,144,224,144,144,144,0], [0,128,128,128,128,128,112,0],
    [0,144,240,240,144,144,144,0], [0,144,208,208,176,176,144,0], [0,96,144,144,144,144,96,0],
    [0,224,144,144,224,128,128,0], [0,96,144,144,144,144,96,48], [0,224,144,144,224,144,144,0],
    [0,112,128,96,16,16,224,0], [0,248,32,32,32,32,32,0], [0,144,144,144,144,144,112,0],
    [0,144,144,144,144,96,96,0], [0,144,144,144,240,240,144,0], [0,144,144,96,96,144,144,0],
    [0,144,144,144,112,16,224,0], [0,240,16,32,64,128,240,0], [112,64,64,64,64,64,64,112],
    [128,128,64,64,32,32,16,16], [112,16,16,16,16,16,16,112], [0,32,80,136,0,0,0,0],
    [0,0,0,0,0,0,0,240], [64,32,0,0,0,0,0,0], [0,0,96,16,112,144,112,0],
    [128,128,224,144,144,144,224,0], [0,0,112,128,128,128,112,0], [16,16,112,144,144,144,112,0],
    [0,0,112,144,240,128,112,0], [48,64,64,224,64,64,64,0], [0,0,112,144,144,96,16,224],
    [128,128,224,144,144,144,144,0], [0,32,0,96,32,32,48,0], [0,32,0,32,32,32,32,192],
    [128,128,144,160,192,160,144,0], [64,64,64,64,64,64,48,0], [0,0,144,240,240,144,144,0],
    [0,0,224,144,144,144,144,0], [0,0,96,144,144,144,96,0], [0,0,224,144,144,224,128,128],
    [0,0,112,144,144,112,16,16], [0,0,112,144,128,128,128,0], [0,0,112,128,96,16,224,0],
    [64,64,224,64,64,64,48,0], [0,0,144,144,144,144,112,0], [0,0,144,144,144,96,96,0],
    [0,0,144,144,240,240,144,0], [0,0,144,96,96,144,144,0], [0,0,144,144,144,112,16,224],
    [0,0,240,16,32,64,240,0], [48,64,64,192,192,64,64,48], [32,32,32,32,32,32,32,32],
    [192,32,32,48,48,32,32,192], [0,0,0,72,176,0,0,0],
];

pub fn glyph(character: char) -> [u8; 8] {
    if (' '..='~').contains(&character) {
        SPLEEN_5X8_ASCII[character as usize - 32]
    } else if character == '↵' {
        [0, 32, 32, 40, 248, 8, 16, 0]
    } else {
        // Fail-closed per-glyph fallback: unsupported Unicode/CJK renders as
        // the explicit box placeholder, never as a real atlas glyph.
        SPLEEN_5X8_UNICODE_FALLBACK_GLYPH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_covers_printable_ascii_and_newline_marker() {
        assert_eq!(SPLEEN_5X8_ASCII.len(), 95);
        assert_eq!(glyph('A'), [0, 96, 144, 144, 240, 144, 144, 0]);
        assert_ne!(glyph('↵'), glyph('?'));
    }

    #[test]
    fn unicode_fallback_is_per_glyph_distinct_and_classified() {
        assert!(is_supported('A'));
        assert!(is_supported('?'));
        assert!(is_supported(' '));
        assert!(is_supported('↵'));
        assert!(!is_supported('中'));
        assert!(!is_supported('文'));
        assert!(!is_supported('\u{2000}'));

        // The fallback box must differ from every real atlas glyph we can
        // confuse it with: '?', 'A', and the newline marker.
        assert_ne!(glyph('中'), glyph('?'));
        assert_ne!(glyph('文'), glyph('A'));
        assert_ne!(glyph('中'), glyph('↵'));
        assert_eq!(glyph('中'), glyph('文'));
        assert_eq!(glyph('中'), SPLEEN_5X8_UNICODE_FALLBACK_GLYPH);
        // Real question marks keep their atlas glyph, so lossy substitution
        // is always visible in the rasterized output.
        assert_eq!(glyph('?'), SPLEEN_5X8_ASCII['?' as usize - 32]);
    }
}
