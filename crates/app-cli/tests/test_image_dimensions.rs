//! Test to verify image dimensions and raster attributes

use terminal_core::sixel::decode_sixel;
use std::fs;

#[test]
fn test_image_dimensions() {
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
    
    // Parse raster attributes manually first
    let s = String::from_utf8_lossy(payload);
    let parts: Vec<&str> = s.split(';').collect();
    println!("Raster attribute parts: {:?}", &parts[0..4.min(parts.len())]);
    
    if parts.len() >= 4 {
        if let Ok(pan) = parts[0].parse::<u32>() {
            if let Ok(pad) = parts[1].parse::<u32>() {
                if let Ok(pw) = parts[2].parse::<u32>() {
                    if let Ok(ph_end) = parts[3].split(|c: char| c == 'q' || c < 0x3F as char).next().unwrap_or("0").parse::<u32>() {
                        println!("Parsed raster attributes:");
                        println!("  Pan (aspect num): {}", pan);
                        println!("  Pad (aspect den): {}", pad);
                        println!("  Pw (width): {}", pw);
                        println!("  Ph (height): {}", ph_end);
                    }
                }
            }
        }
    }
    
    // Now decode with the decoder
    let sixel_img = decode_sixel(payload).expect("Failed to decode sixel");
    println!("\nDecoded image dimensions: {}x{}", sixel_img.width, sixel_img.height);
    println!("Decoded image data size: {} bytes", sixel_img.rgba.len());
    println!("Expected size ({}x{} * 4): {} bytes", sixel_img.width, sixel_img.height, sixel_img.width * sixel_img.height * 4);
    
    // Verify consistency
    let expected_pixels = (sixel_img.width as usize) * (sixel_img.height as usize);
    let actual_pixels = sixel_img.rgba.len() / 4;
    println!("\nPixel count verification:");
    println!("  Expected: {}", expected_pixels);
    println!("  Actual: {}", actual_pixels);
    println!("  Match: {}", expected_pixels == actual_pixels);
}
