//! Test to debug sixel parsing and color detection

use std::fs;
use terminal_core::sixel::decode_sixel;

#[test]
fn test_sixel_parsing() {
    let sixel_data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Skip ESC P q prefix to get to the payload
    let payload = if sixel_data.len() > 3 && sixel_data[0] == 0x1b && sixel_data[1] == b'P' && sixel_data[2] == b'q' {
        &sixel_data[3..]
    } else {
        &sixel_data
    };
    
    // Find the end ESC \ (should be last 2 bytes)
    let payload = if payload.len() >= 2 && payload[payload.len()-2] == 0x1b && payload[payload.len()-1] == b'\\' {
        &payload[..payload.len()-2]
    } else {
        payload
    };
    
    println!("Payload length: {} bytes", payload.len());
    
    // Count different types of bytes
    let mut has_raster = false;
    let mut has_color = false;
    let mut sixel_byte_count = 0;
    let mut digit_count = 0;
    let mut other_count = 0;
    let mut color_select_count = 0;
    
    for &b in payload {
        if b == b'"' {
            has_raster = true;
        } else if b == b'#' {
            has_color = true;
        } else if b >= 0x3F && b <= 0x7E {
            sixel_byte_count += 1;
        } else if b.is_ascii_digit() {
            digit_count += 1;
            if digit_count <= 20 {
                print!("{}", (b - b'0') as i32);
                if digit_count % 10 == 0 {
                    print!(" ");
                }
            }
        } else if b == b'-' || b == b'$' || b == b'!' {
            other_count += 1;
        } else if b < 32 || b >= 127 {
            // Skip unprintable
        } else {
            color_select_count += 1;
        }
    }
    
    println!("\n\nAnalysis:");
    println!("  Raster attributes present: {}", has_raster);
    println!("  Color definitions present: {}", has_color);
    println!("  Sixel data bytes (0x3F-0x7E): {}", sixel_byte_count);
    println!("  Digit characters: {}", digit_count);
    println!("  Control characters ($,-,!): {}", other_count);
    println!("  Color select commands: {}", color_select_count);
    
    // Now decode
    let result = decode_sixel(payload).expect("Failed to decode sixel");
    println!("\nDecoded dimensions: {}x{}", result.width, result.height);
    println!("RGBA buffer size: {} bytes", result.rgba.len());
    
    // Analyze the palette by checking what color indices appear in the decoded data
    let mut color_indices = std::collections::HashSet::new();
    for chunk in result.rgba.chunks(4) {
        if chunk.len() >= 4 {
            let [r, g, b, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            // Skip pure white (255, 255, 255)
            if !(r == 255 && g == 255 && b == 255) {
                color_indices.insert((r, g, b, a));
            }
        }
    }
    
    println!("Unique colored pixels found: {}", color_indices.len());
    let mut colors: Vec<_> = color_indices.iter().collect();
    colors.sort();
    for (r, g, b, a) in colors.iter().take(20) {
        println!("  RGBA({}, {}, {}, {})", r, g, b, a);
    }
}
