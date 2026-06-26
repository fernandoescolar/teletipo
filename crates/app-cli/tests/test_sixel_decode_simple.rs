//! Test sixel decoder with a simple known pattern

use terminal_core::sixel::decode_sixel;

#[test]
fn test_decode_simple_sixel_pattern() {
    // Create a simple sixel: just a few pixels
    // Format: raster attributes, then color defs, then sixel data
    
    // Let's create: 10x6 image with a simple pattern
    // Raster attr: "1;1;10;6 q" (10 width, 6 height)
    // Color 0: #0;2;0;0;0 (black)
    // Color 1: #1;2;0;100;0 (green in HLS)
    // Sixel data: one row of 0x3F (all bits set = all pixels color 1, except we need to select colors)
    
    let sixel_data = b"1;1;10;6q#0;2;0;0;0#1;2;0;100;0#1!10?-";
    
    match terminal_core::sixel::decode_sixel(sixel_data) {
        Ok(img) => {
            println!("Decoded: {}x{}", img.width, img.height);
            println!("RGBA buffer size: {} bytes ({} pixels)", img.rgba.len(), img.rgba.len() / 4);
            
            // Check first pixel
            if img.rgba.len() >= 4 {
                let r = img.rgba[0];
                let g = img.rgba[1];
                let b = img.rgba[2];
                let a = img.rgba[3];
                println!("First pixel: RGBA({}, {}, {}, {})", r, g, b, a);
            }
            
            // Print pattern
            println!("\nPattern visualization (first row):");
            for x in 0..10 {
                if x * 4 + 3 < img.rgba.len() {
                    let r = img.rgba[x * 4];
                    let g = img.rgba[x * 4 + 1];
                    let b = img.rgba[x * 4 + 2];
                    
                    if r == 0 && g == 0 && b == 0 {
                        print!("█");  // black
                    } else if g > 100 {
                        print!("G");  // green
                    } else {
                        print!("?");  // unknown
                    }
                }
            }
            println!();
        }
        Err(e) => println!("Error: {}", e),
    }
}

#[test]
fn test_pixel_memory_layout() {
    // Verify RGBA memory layout is correct
    use terminal_core::sixel::decode_sixel;
    use std::fs;
    
    let sixel_data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    let payload = if sixel_data.len() > 3 && sixel_data[0] == 0x1b && sixel_data[1] == b'P' && sixel_data[2] == b'q' {
        &sixel_data[3..]
    } else {
        &sixel_data
    };
    
    let payload = if payload.len() >= 2 && payload[payload.len()-2] == 0x1b && payload[payload.len()-1] == b'\\' {
        &payload[..payload.len()-2]
    } else {
        payload
    };
    
    let img = decode_sixel(payload).expect("Failed to decode");
    
    println!("Image: {}x{}, {} bytes", img.width, img.height, img.rgba.len());
    
    // Check if bytes are in RGBA order (not ARGB or BGR)
    let mut rgba_count = 0;
    let mut other_count = 0;
    
    for i in (0..img.rgba.len()).step_by(4) {
        if i + 3 < img.rgba.len() {
            let r = img.rgba[i] as u32;
            let g = img.rgba[i + 1] as u32;
            let b = img.rgba[i + 2] as u32;
            let a = img.rgba[i + 3] as u32;
            
            // Valid RGBA: should have reasonable distribution
            if !(r == 255 && g == 255 && b == 255 && a == 255) {
                rgba_count += 1;
            } else {
                other_count += 1;
            }
        }
    }
    
    println!("Non-white pixels: {}", rgba_count);
    println!("White pixels: {}", other_count);
    
    // Check first 10 non-white pixels
    println!("\nFirst 10 colored pixels (checking byte order):");
    let mut count = 0;
    for i in (0..img.rgba.len()).step_by(4) {
        if i + 3 < img.rgba.len() {
            let r = img.rgba[i];
            let g = img.rgba[i + 1];
            let b = img.rgba[i + 2];
            let a = img.rgba[i + 3];
            
            if !(r == 255 && g == 255 && b == 255) {
                println!("  Pixel {}: byte[{}] R={} G={} B={} A={}", count, i, r, g, b, a);
                count += 1;
                if count >= 10 {
                    break;
                }
            }
        }
    }
}
