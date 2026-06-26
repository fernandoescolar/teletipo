//! Test to visualize what the decoder is producing

use terminal_core::sixel::decode_sixel;
use std::fs;

#[test]
fn test_image_visual_text_dump() {
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
    
    println!("Image dimensions: {}x{}", width, height);
    println!();
    
    // Show top 30 rows as ASCII art
    println!("TOP 30 ROWS (as would appear on screen):");
    for row in 0..30.min(height) {
        for col in 0..100.min(width) {
            let idx = (row * width + col) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            
            // Simple color to ASCII conversion
            if r == 255 && g == 255 && b == 255 {
                print!(" ");
            } else if r < 100 && g < 100 && b < 100 {
                print!("█");
            } else {
                print!("▓");
            }
        }
        println!();
    }
    
    println!();
    println!("BOTTOM 30 ROWS (as would appear on screen):");
    let start_row = (height - 30).max(0);
    for row in start_row..height {
        for col in 0..100.min(width) {
            let idx = (row * width + col) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            
            if r == 255 && g == 255 && b == 255 {
                print!(" ");
            } else if r < 100 && g < 100 && b < 100 {
                print!("█");
            } else {
                print!("▓");
            }
        }
        println!();
    }
}
