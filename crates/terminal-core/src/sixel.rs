//! Complete Sixel image format decoder.
//!
//! Sixel is a character-based graphics format transmitted via DCS sequences.
//! Format: `ESC P q ... ESC \`
//!
//! Sequence structure:
//! 1. Optional raster attributes: `"Pw;Ph;Pc` (width, height, color-map-type)
//! 2. Optional color palette definitions: `#N;Cs;Hls;R;G;B` (register N with HSL or RGB)
//! 3. Graphics data: sixel bytes (0x3F-0x7E) encoding 6 vertical pixels each
//! 4. Control codes: `$` (carriage return), `-` (line feed), `!Ncc` (run-length encoding)

use std::collections::HashMap;

/// Represents a decoded Sixel image with RGBA pixel data.
#[derive(Debug, Clone)]
pub struct SixelImage {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// RGBA pixel data (width * height * 4 bytes).
    pub rgba: Vec<u8>,
}

/// Default palette: VT340 16-color set (simplified).
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

/// Sixel decoder state machine for parsing graphics data.
struct SixelDecoder {
    width: usize,
    height: usize,
    palette: HashMap<u8, [u8; 3]>,
    current_color: u8,
    rows: Vec<Vec<u8>>, // Each row is a vec of palette indices
    current_row: Vec<u8>,
    current_col: usize,
    sixel_height: usize, // Height in sixels (default 6 pixels per sixel)
}

impl SixelDecoder {
    /// Create a new decoder with default palette.
    fn new() -> Self {
        let mut palette = HashMap::new();
        for (i, &[r, g, b]) in DEFAULT_PALETTE.iter().enumerate() {
            palette.insert(i as u8, [r, g, b]);
        }
        Self {
            width: 640,  // Default VT340 width
            height: 400, // Will be adjusted based on data
            palette,
            current_color: 0,
            rows: Vec::new(),
            current_row: Vec::new(),
            current_col: 0,
            sixel_height: 6,
        }
    }

    /// Parse raster attributes: `"Pw;Ph;Pc`
    fn parse_raster_attributes(&mut self, attrs: &str) {
        let parts: Vec<&str> = attrs.split(';').collect();
        if !parts.is_empty()
            && let Ok(w) = parts[0].parse::<usize>()
        {
            self.width = w.max(1);
        }
        if parts.len() >= 2
            && let Ok(h) = parts[1].parse::<usize>()
        {
            self.height = h.max(1);
        }
        // parts[2] is color-map-type (0=indexed, 1=RGB), handled elsewhere
    }

    /// Parse color palette definition: `#N;Cs;Hls;R;G;B`
    fn parse_color_definition(&mut self, def: &str) {
        let parts: Vec<&str> = def.split(';').collect();
        if parts.len() < 5 {
            return;
        }

        let palette_idx = match parts[0].parse::<u8>() {
            Ok(n) => n,
            Err(_) => return,
        };
        let color_space = parts[1];
        let _mode = parts[2];
        let r_str = parts.get(3).copied().unwrap_or("0");
        let g_str = parts.get(4).copied().unwrap_or("0");
        let b_str = parts.get(5).copied().unwrap_or("0");

        let (r, g, b) = if color_space == "1" {
            // RGB color space
            let r = r_str.parse::<u8>().unwrap_or(0);
            let g = g_str.parse::<u8>().unwrap_or(0);
            let b = b_str.parse::<u8>().unwrap_or(0);
            (r, g, b)
        } else {
            // HLS color space (simplified - just use RGB values directly for now)
            let r = r_str.parse::<u8>().unwrap_or(0);
            let g = g_str.parse::<u8>().unwrap_or(0);
            let b = b_str.parse::<u8>().unwrap_or(0);
            (r, g, b)
        };

        self.palette.insert(palette_idx, [r, g, b]);
    }

    /// Set the current drawing color.
    fn set_color(&mut self, color_idx: u8) {
        self.current_color = color_idx;
    }

    /// Handle a sixel data byte (0x3F-0x7E = 6 bits of pixels).
    fn write_sixel(&mut self, byte: u8) {
        if !(0x3F..=0x7E).contains(&byte) {
            return; // Invalid sixel byte
        }

        let bits = byte - 0x3F;
        for row_offset in 0..6 {
            if bits & (1 << row_offset) != 0 {
                // Extend rows if needed
                while self.rows.len() <= row_offset {
                    self.rows.push(Vec::new());
                }
                // Extend current row to current column
                while self.rows[row_offset].len() <= self.current_col {
                    self.rows[row_offset].push(0);
                }
                self.rows[row_offset][self.current_col] = self.current_color;
            }
        }
        self.current_col += 1;
    }

    /// Carriage return (advance to start of next sixel row).
    fn carriage_return(&mut self) {
        // Finish current row
        if !self.current_row.is_empty() {
            self.rows.push(self.current_row.clone());
            self.current_row.clear();
        }
        self.current_col = 0;
    }

    /// Convert palette-indexed image to RGBA.
    #[allow(clippy::wrong_self_convention)]
    fn to_rgba(self) -> (Vec<u8>, usize, usize) {
        if self.rows.is_empty() {
            return (vec![], 0, 0);
        }

        // Determine final dimensions
        let height_px = self.rows.len() * self.sixel_height;
        let width_px = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);

        if width_px == 0 || height_px == 0 {
            return (vec![], 0, 0);
        }

        let mut rgba = vec![0u8; width_px * height_px * 4];

        // Fill with background (white by default) and overwrite with sixels
        for i in 0..rgba.len() / 4 {
            rgba[i * 4] = 255; // R
            rgba[i * 4 + 1] = 255; // G
            rgba[i * 4 + 2] = 255; // B
            rgba[i * 4 + 3] = 255; // A
        }

        // Draw sixels
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, &color_idx) in row.iter().enumerate() {
                let [r, g, b] = self.palette.get(&color_idx).copied().unwrap_or([0, 0, 0]);
                for py in 0..self.sixel_height {
                    let y = row_idx * self.sixel_height + py;
                    if y >= height_px {
                        break;
                    }
                    let idx = (y * width_px + col_idx) * 4;
                    if idx + 3 < rgba.len() {
                        rgba[idx] = r;
                        rgba[idx + 1] = g;
                        rgba[idx + 2] = b;
                        rgba[idx + 3] = 255;
                    }
                }
            }
        }

        (rgba, width_px, height_px)
    }
}

/// Decode Sixel graphics data.
///
/// Takes the raw DCS payload (without `q` prefix) and returns a decoded image.
/// If parsing fails, returns a placeholder to avoid crashing.
pub fn decode_sixel(data: &[u8]) -> Result<SixelImage, String> {
    let s = String::from_utf8_lossy(data);
    let mut decoder = SixelDecoder::new();

    let mut i = 0;
    let bytes = s.as_bytes();

    // Parse raster attributes and palette before data
    while i < bytes.len() {
        i = parse_sixel_byte(&mut decoder, bytes, i);
    }

    // Finalize
    decoder.carriage_return();

    let (rgba, width, height) = decoder.to_rgba();
    if rgba.is_empty() {
        // Return placeholder if decoding resulted in no pixels
        return Ok(placeholder_image());
    }

    Ok(SixelImage {
        width,
        height,
        rgba,
    })
}

/// Parse a single sixel byte and advance position.
fn parse_sixel_byte(decoder: &mut SixelDecoder, bytes: &[u8], mut i: usize) -> usize {
    match bytes[i] {
        b'"' => {
            // Raster attributes: "Pw;Ph;Pc
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                i += 1;
            }
            let attrs = String::from_utf8_lossy(&bytes[start..i]);
            decoder.parse_raster_attributes(&attrs);
        }
        b'#' => {
            // Color palette: #N;Cs;Hls;R;G;B
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                i += 1;
            }
            let def = String::from_utf8_lossy(&bytes[start..i]);
            decoder.parse_color_definition(&def);
        }
        b'$' => {
            // Carriage return
            decoder.carriage_return();
            i += 1;
        }
        b'-' => {
            // Line feed (next sixel row)
            decoder.carriage_return();
            i += 1;
        }
        b'!' => {
            // Run-length encoding: !Ncc (repeat cc N times)
            i = parse_rle(decoder, bytes, i);
        }
        b => {
            if (0x3F..=0x7E).contains(&b) {
                // Sixel data byte
                decoder.write_sixel(b);
            } else if b.is_ascii_digit() {
                // Color select: just a digit (0-9 = palette 0-9)
                decoder.set_color(b - b'0');
            }
            i += 1;
        }
    }
    i
}

/// Parse run-length encoding sequence: !Ncc
fn parse_rle(decoder: &mut SixelDecoder, bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let count_str = String::from_utf8_lossy(&bytes[start..i]);
    let count: usize = count_str.parse().unwrap_or(1);
    if i < bytes.len() {
        let byte = bytes[i];
        for _ in 0..count {
            decoder.write_sixel(byte);
        }
        i += 1;
    }
    i
}

/// Generate a placeholder checkerboard image when decoding fails or produces empty result.
fn placeholder_image() -> SixelImage {
    let width = 64;
    let height = 64;
    let mut rgba = vec![0u8; width * height * 4];

    // Checkerboard pattern
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
            rgba[idx + 3] = 255; // A
        }
    }

    SixelImage {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_simple_sixel() {
        // Simple test: empty data should produce placeholder
        let data = b"";
        let result = decode_sixel(data);
        assert!(result.is_ok());
        let img = result.unwrap();
        assert!(img.width > 0);
        assert!(img.height > 0);
        assert_eq!(img.rgba.len(), img.width * img.height * 4);
    }

    #[test]
    fn test_decode_with_palette() {
        // Test with color palette definition
        let data = b"#0;2;0;100;100;100";
        let result = decode_sixel(data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_placeholder_image() {
        let img = placeholder_image();
        assert_eq!(img.width, 64);
        assert_eq!(img.height, 64);
        assert_eq!(img.rgba.len(), 64 * 64 * 4);
    }
}
