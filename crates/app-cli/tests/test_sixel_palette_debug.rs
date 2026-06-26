//! Test to debug sixel palette and pixel writing

use std::fs;
use terminal_core::sixel::decode_sixel;

#[test]
fn test_sixel_palette_debug() {
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
    
    // Parse just the first few color definitions manually
    let s = String::from_utf8_lossy(payload);
    let lines: Vec<&str> = s.split('#').collect();
    
    println!("Found {} color definition sections", lines.len());
    println!("\nFirst 15 color definitions:");
    for (i, line) in lines.iter().take(16).enumerate() {
        if i == 0 {
            // First element is before any '#'
            if line.contains(';') {
                println!("[Raster attributes] {}", line.split(';').take(4).collect::<Vec<_>>().join(";"));
            }
        } else {
            // Split at first sixel byte or control character
            let color_part = line.split(|c: char| c == '-' || c == '$' || (c as u8 >= 0x3F && c as u8 <= 0x7E)).next().unwrap_or("");
            if !color_part.is_empty() {
                let parts: Vec<&str> = color_part.split(';').collect();
                if parts.len() >= 2 {
                    let color_space = parts.get(1).copied().unwrap_or("?");
                    let v1 = parts.get(2).copied().unwrap_or("?");
                    let v2 = parts.get(3).copied().unwrap_or("?");
                    let v3 = parts.get(4).copied().unwrap_or("?");
                    
                    if color_space == "2" {
                        println!("  Color #{}: HLS({}, {}, {})", parts[0], v1, v2, v3);
                    } else {
                        println!("  Color #{}: Cs={} ({}, {}, {})", parts[0], color_space, v1, v2, v3);
                    }
                }
            }
        }
    }
    
    // Now decode and check
    let result = decode_sixel(payload).expect("Failed to decode sixel");
    println!("\n\nDecoded dimensions: {}x{}", result.width, result.height);
    
    // Check some specific pixels
    println!("\nSample pixels from decoded image:");
    println!("  Pixel [0,0]: {:?}", [result.rgba[0], result.rgba[1], result.rgba[2], result.rgba[3]]);
    println!("  Pixel [100,0]: {:?}", [result.rgba[(100*4)], result.rgba[(100*4)+1], result.rgba[(100*4)+2], result.rgba[(100*4)+3]]);
    println!("  Pixel [300,100]: {:?}", [result.rgba[(100*600*4)+(300*4)], result.rgba[(100*600*4)+(300*4)+1], result.rgba[(100*600*4)+(300*4)+2], result.rgba[(100*600*4)+(300*4)+3]]);
    
    // Check if ANY non-white pixels exist
    let mut has_non_white = false;
    for chunk in result.rgba.chunks(4).take(1000) {
        if chunk.len() >= 4 {
            if !(chunk[0] == 255 && chunk[1] == 255 && chunk[2] == 255) {
                has_non_white = true;
                println!("Found non-white pixel: RGBA({}, {}, {}, {})", chunk[0], chunk[1], chunk[2], chunk[3]);
                break;
            }
        }
    }
    
    if !has_non_white {
        println!("No non-white pixels found in first 1000 pixels");
    }
}
