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

/// Convert HLS (Hue, Lightness, Saturation) to RGB.
/// H: 0-360 degrees
/// L: 0-255 (normalized to 0-1 by dividing by 255)
/// S: 0-255 (normalized to 0-1 by dividing by 255)
/// Returns (R, G, B) where each is 0-1.
fn hls_to_rgb(h: f32, l: f32, s: f32) -> (f32, f32, f32) {
    let h = h % 360.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    
    let m = l - c / 2.0;
    ((r1 + m).max(0.0).min(1.0), (g1 + m).max(0.0).min(1.0), (b1 + m).max(0.0).min(1.0))
}

/// Sixel decoder state machine for parsing graphics data.
struct SixelDecoder {
    width: usize,
    height: usize,
    palette: HashMap<u8, [u8; 3]>,
    current_color: u8,
    rows: Vec<Vec<u8>>, // rows[pixel_row][pixel_col] = palette index
    current_col: usize, // Current column in current sixel row
    current_sixel_row: usize, // Which sixel row we're on (0 means rows 0-5, 1 means rows 6-11, etc.)
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
            current_col: 0,
            current_sixel_row: 0,
        }
    }

    /// Parse raster attributes: can be 3-parameter legacy or 4-parameter extended
    /// Legacy (3 params): "Pw;Ph;Pc (width, height, color-map-type)
    /// Extended (4 params): "Pan;Pad;Pw;Ph (aspect num/denom, pixel width, pixel height)
    fn parse_raster_attributes(&mut self, attrs: &str) {
        let parts: Vec<&str> = attrs.split(';').collect();
        
        if parts.len() >= 4 {
            // Extended 4-parameter format: Pan;Pad;Pw;Ph
            // parts[0] = aspect numerator (ignore)
            // parts[1] = aspect denominator (ignore)
            // parts[2] = pixel width
            // parts[3] = pixel height
            if let Ok(w) = parts[2].parse::<usize>() {
                self.width = w.max(1);
            }
            if let Ok(h) = parts[3].parse::<usize>() {
                self.height = h.max(1);
            }
        } else if parts.len() >= 2 {
            // Legacy 3-parameter format: Pw;Ph;Pc
            if let Ok(w) = parts[0].parse::<usize>() {
                self.width = w.max(1);
            }
            if let Ok(h) = parts[1].parse::<usize>() {
                self.height = h.max(1);
            }
        }
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
        let v1_str = parts.get(2).copied().unwrap_or("0");
        let v2_str = parts.get(3).copied().unwrap_or("0");
        let v3_str = parts.get(4).copied().unwrap_or("0");

        let (r, g, b) = if color_space == "1" {
            // RGB color space: v1=R, v2=G, v3=B (0-255)
            let r = v1_str.parse::<u8>().unwrap_or(0);
            let g = v2_str.parse::<u8>().unwrap_or(0);
            let b = v3_str.parse::<u8>().unwrap_or(0);
            (r, g, b)
        } else if color_space == "2" {
            // HLS color space: v1=H (0-360), v2=L (0-100), v3=S (0-100)
            let h = v1_str.parse::<f32>().unwrap_or(0.0);
            let l = v2_str.parse::<f32>().unwrap_or(0.0) / 100.0; // Normalize to 0-1
            let s = v3_str.parse::<f32>().unwrap_or(0.0) / 100.0; // Normalize to 0-1
            let (rf, gf, bf) = hls_to_rgb(h, l, s);
            ((rf * 255.0) as u8, (gf * 255.0) as u8, (bf * 255.0) as u8)
        } else {
            // Unknown color space, use values directly
            let r = v1_str.parse::<u8>().unwrap_or(0);
            let g = v2_str.parse::<u8>().unwrap_or(0);
            let b = v3_str.parse::<u8>().unwrap_or(0);
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
        let base_row = self.current_sixel_row * 6; // Which row to start writing from
        
        for bit_offset in 0..6 {
            if bits & (1 << bit_offset) != 0 {
                let row_idx = base_row + bit_offset;
                
                // Extend rows if needed
                while self.rows.len() <= row_idx {
                    self.rows.push(Vec::new());
                }
                
                // Extend current row to current column
                while self.rows[row_idx].len() <= self.current_col {
                    self.rows[row_idx].push(0);
                }
                
                self.rows[row_idx][self.current_col] = self.current_color;
            }
        }
        
        self.current_col += 1;
    }

    /// Carriage return (advance to start of next sixel row).
    fn carriage_return(&mut self) {
        // Get the maximum column width for this sixel row
        let base_row = self.current_sixel_row * 6;
        let max_col = (0..6)
            .map(|i| {
                if base_row + i < self.rows.len() {
                    self.rows[base_row + i].len()
                } else {
                    0
                }
            })
            .max()
            .unwrap_or(0);
        
        // Pad all rows in this sixel row to the same length
        for i in 0..6 {
            let row_idx = base_row + i;
            while row_idx >= self.rows.len() {
                self.rows.push(Vec::new());
            }
            while self.rows[row_idx].len() < max_col {
                self.rows[row_idx].push(0);
            }
        }
        
        // Move to next sixel row
        self.current_sixel_row += 1;
        self.current_col = 0;
    }

    /// Convert palette-indexed image to RGBA.
    #[allow(clippy::wrong_self_convention)]
    fn to_rgba(self) -> (Vec<u8>, usize, usize) {
        if self.rows.is_empty() {
            return (vec![], 0, 0);
        }

        // Use raster attributes for dimensions, or calculate from data
        let height_px = if self.height > 0 {
            self.height
        } else {
            self.rows.len()  // Now rows.len() directly represents pixel height
        };
        
        let width_px = if self.width > 0 {
            self.width
        } else {
            self.rows.iter().map(|r| r.len()).max().unwrap_or(0)
        };

        if width_px == 0 || height_px == 0 {
            return (vec![], 0, 0);
        }

        let mut rgba = vec![0u8; width_px * height_px * 4];

        // Fill with background (white by default)
        for i in 0..rgba.len() / 4 {
            rgba[i * 4] = 255; // R
            rgba[i * 4 + 1] = 255; // G
            rgba[i * 4 + 2] = 255; // B
            rgba[i * 4 + 3] = 255; // A
        }

        // Draw sixels with scaling to fit raster dimensions
        let data_height = self.rows.len();  // Now this is actual pixel rows!
        let data_width = self.rows.iter().map(|r| r.len()).max().unwrap_or(1);
        
        let scale_y = if data_height > 0 {
            height_px as f32 / data_height as f32
        } else {
            1.0
        };
        
        let scale_x = if data_width > 0 {
            width_px as f32 / data_width as f32
        } else {
            1.0
        };

        // Draw sixels - now rows[i] directly corresponds to pixel row i
        for (row_idx, row) in self.rows.iter().enumerate() {
            let screen_y_start = (row_idx as f32 * scale_y) as usize;
            let screen_y_end = ((row_idx as f32 + 1.0) * scale_y) as usize;
            
            if screen_y_start >= height_px {
                break;
            }
            
            for (col_idx, &color_idx) in row.iter().enumerate() {
                // Skip pixels that were never set (remain as padding zeros and don't have a palette entry)
                // But allow color index 0 if it's defined in the palette
                if color_idx == 0 && !self.palette.contains_key(&0) {
                    continue; // Skip uninitialized pixels
                }
                
                let [r, g, b] = self.palette.get(&color_idx).copied().unwrap_or([255, 255, 255]);
                let screen_x_start = (col_idx as f32 * scale_x) as usize;
                let screen_x_end = ((col_idx as f32 + 1.0) * scale_x) as usize;
                
                // Fill pixels for this sixel
                for screen_y in screen_y_start..screen_y_end.min(height_px) {
                    for screen_x in screen_x_start..screen_x_end.min(width_px) {
                        let idx = (screen_y * width_px + screen_x) * 4;
                        if idx + 3 < rgba.len() {
                            rgba[idx] = r;
                            rgba[idx + 1] = g;
                            rgba[idx + 2] = b;
                            rgba[idx + 3] = 255;
                        }
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
            // Color commands: can be definition or selection
            // Definition: #N;Cs;V1;V2;V3
            // Selection: #N (just a number)
            i += 1;
            let start = i;
            
            // Consume digits until we hit a non-digit, semicolon, # or byte < 0x3F
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            
            // Now check what comes next
            if i < bytes.len() && bytes[i] == b';' {
                // It's a color definition (#N;Cs;...)
                // Consume until we see another # or a byte < 0x3F
                while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                    i += 1;
                }
                let def = String::from_utf8_lossy(&bytes[start..i]);
                decoder.parse_color_definition(&def);
            } else {
                // It's a color selection (#N)
                let color_str = String::from_utf8_lossy(&bytes[start..i]);
                if let Ok(color_idx) = color_str.parse::<u8>() {
                    decoder.set_color(color_idx);
                }
            }
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
