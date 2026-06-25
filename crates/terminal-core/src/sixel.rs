//! Basic Sixel image format decoder.
//!
//! Sixel is a character-based graphics format transmitted via DCS sequences.
//! Format: `ESC P q ... ESC \`
//!
//! Structure:
//! - Color palette definition: `#N;Cs;Hls;R;G;B` (register N with HSL or RGB)
//! - Raster attribute: `"Pw;Ph;Pc` (width, height, color-map-type)
//! - Graphics data: sixel bytes (0x3F-0x7E encode 6 vertical pixels each)
//! - Carriage return: `$` (advance to next column without moving vertically)
//! - Line feed: `-` (move to next sixel row)

/// Represents a decoded Sixel image.
#[derive(Debug, Clone)]
pub struct SixelImage {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// RGBA pixel data (width * height * 4 bytes).
    pub rgba: Vec<u8>,
}

/// Default palette (16 VT340 colors, simplified).
#[allow(dead_code)]
const DEFAULT_PALETTE: &[[u8; 3]] = &[
    [0, 0, 0],       // 0: black
    [128, 0, 0],     // 1: red
    [0, 128, 0],     // 2: green
    [128, 128, 0],   // 3: yellow
    [0, 0, 128],     // 4: blue
    [128, 0, 128],   // 5: magenta
    [0, 128, 128],   // 6: cyan
    [192, 192, 192], // 7: white
    [128, 128, 128], // 8: bright black
    [255, 0, 0],     // 9: bright red
    [0, 255, 0],     // 10: bright green
    [255, 255, 0],   // 11: bright yellow
    [0, 0, 255],     // 12: bright blue
    [255, 0, 255],   // 13: bright magenta
    [0, 255, 255],   // 14: bright cyan
    [255, 255, 255], // 15: bright white
];

/// Minimal Sixel decoder that extracts basic image data.
/// For now, returns a placeholder image to avoid crashing.
pub fn decode_sixel(_data: &[u8]) -> Result<SixelImage, String> {
    // TODO: implement full sixel decoding
    // For now, return a small placeholder to prove the integration works
    let width = 64;
    let height = 64;
    let mut rgba = vec![0u8; width * height * 4];

    // Fill with a simple checkerboard pattern
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let is_dark = ((x / 8) + (y / 8)) % 2 == 0;
            if is_dark {
                rgba[idx] = 64; // R
                rgba[idx + 1] = 64; // G
                rgba[idx + 2] = 64; // B
            } else {
                rgba[idx] = 192; // R
                rgba[idx + 1] = 192; // G
                rgba[idx + 2] = 192; // B
            }
            rgba[idx + 3] = 255; // A (fully opaque)
        }
    }

    Ok(SixelImage {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_placeholder() {
        let data = b"q";
        let result = decode_sixel(data);
        assert!(result.is_ok());
        let img = result.unwrap();
        assert_eq!(img.width, 64);
        assert_eq!(img.height, 64);
        assert_eq!(img.rgba.len(), 64 * 64 * 4);
    }
}
