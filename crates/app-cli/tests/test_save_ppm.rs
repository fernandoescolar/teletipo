//! Test to save decoded sixel image as PPM file for visual inspection

use terminal_core::sixel::decode_sixel;
use std::fs::{self, File};
use std::io::Write;

#[test]
fn test_save_sixel_as_ppm() {
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
    
    let sixel_img = decode_sixel(payload).expect("Failed to decode sixel");
    let (rgba, width, height) = (sixel_img.rgba, sixel_img.width as usize, sixel_img.height as usize);
    
    // Create PPM file
    let mut ppm = File::create("/tmp/snake.ppm").expect("Failed to create PPM");
    
    // PPM header
    write!(ppm, "P6\n{} {}\n255\n", width, height).expect("Failed to write PPM header");
    
    // Convert RGBA to RGB for PPM
    for i in (0..rgba.len()).step_by(4) {
        let r = rgba[i];
        let g = rgba[i + 1];
        let b = rgba[i + 2];
        ppm.write_all(&[r, g, b]).expect("Failed to write pixel");
    }
    
    println!("Saved decoded sixel image to /tmp/snake.ppm");
    println!("Dimensions: {}x{}", width, height);
    println!("File size: {} bytes", 14 + width * height * 3);
    
    // Also check for patterns
    let mut row_profiles = vec![];
    for row in 0..height.min(50) {
        let mut colored_count = 0;
        for col in 0..width {
            let idx = (row * width + col) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            if !(r == 255 && g == 255 && b == 255) {
                colored_count += 1;
            }
        }
        row_profiles.push(colored_count);
    }
    
    println!("\nFirst 50 rows (colored pixel count):");
    for (row, count) in row_profiles.iter().enumerate() {
        println!("  Row {}: {} pixels", row, count);
    }
}
