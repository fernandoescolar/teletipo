//! Debug test to check sixel decoding details

use std::fs;

#[test]
fn test_sixel_parsing_debug() {
    let sixel_data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Skip ESC P q prefix
    let payload = if sixel_data.len() > 3 && sixel_data[0] == 0x1b && sixel_data[1] == b'P' && sixel_data[2] == b'q' {
        &sixel_data[3..]
    } else {
        &sixel_data
    };
    
    // Skip the end marker
    let payload = if payload.len() >= 2 && payload[payload.len()-2] == 0x1b && payload[payload.len()-1] == b'\\' {
        &payload[..payload.len()-2]
    } else {
        payload
    };
    
    println!("Total payload: {} bytes", payload.len());
    
    // Parse manually to see what we're getting
    let mut raster_attrs = String::new();
    let mut color_defs = 0;
    let mut sixel_data_count = 0;
    let mut cr_count = 0;
    let mut lf_count = 0;
    let mut current_color = 0u8;
    let mut color_changes = 0;
    
    let s = String::from_utf8_lossy(payload);
    let bytes = s.as_bytes();
    let mut i = 0;
    
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                    i += 1;
                }
                raster_attrs = String::from_utf8_lossy(&bytes[start..i]).to_string();
            }
            b'#' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                
                if i < bytes.len() && bytes[i] == b';' {
                    color_defs += 1;
                    while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                        i += 1;
                    }
                } else {
                    let color_str = String::from_utf8_lossy(&bytes[start..i]);
                    if let Ok(new_color) = color_str.parse::<u8>() {
                        if new_color != current_color {
                            color_changes += 1;
                            current_color = new_color;
                        }
                    }
                }
            }
            b'$' => {
                cr_count += 1;
                i += 1;
            }
            b'-' => {
                lf_count += 1;
                i += 1;
            }
            b'!' => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && (0x3F..=0x7E).contains(&bytes[i]) {
                    sixel_data_count += 1;
                    i += 1;
                }
            }
            b => {
                if (0x3F..=0x7E).contains(&b) {
                    sixel_data_count += 1;
                } else if b.is_ascii_digit() {
                    if (b - b'0') != current_color {
                        color_changes += 1;
                        current_color = b - b'0';
                    }
                }
                i += 1;
            }
        }
    }
    
    println!("Raster attributes: {}", raster_attrs);
    println!("Color definitions: {}", color_defs);
    println!("Sixel data bytes: {}", sixel_data_count);
    println!("Carriage returns ($): {}", cr_count);
    println!("Line feeds (-): {}", lf_count);
    println!("Color changes: {}", color_changes);
}
