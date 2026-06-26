//! Test to check if image rows need vertical flipping

use terminal_core::sixel::decode_sixel;
use std::fs;

#[test]
fn test_image_vertical_orientation() {
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
    
    println!("Checking image vertical orientation:");
    println!("Image dimensions: {}x{}", width, height);
    println!();
    
    // Check first row (top of image as stored)
    println!("First row (row 0, top):");
    let mut non_white_count = 0;
    for x in 0..width.min(50) {
        let idx = (0 * width + x) * 4;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        let b = rgba[idx + 2];
        if !(r == 255 && g == 255 && b == 255) {
            non_white_count += 1;
            print!("█");
        } else {
            print!(" ");
        }
    }
    println!(" ({} non-white pixels)", non_white_count);
    
    // Check last row (bottom of image as stored)
    println!("\nLast row (row {}, bottom):", height - 1);
    let mut non_white_count = 0;
    for x in 0..width.min(50) {
        let idx = ((height - 1) * width + x) * 4;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        let b = rgba[idx + 2];
        if !(r == 255 && g == 255 && b == 255) {
            non_white_count += 1;
            print!("█");
        } else {
            print!(" ");
        }
    }
    println!(" ({} non-white pixels)", non_white_count);
    
    // Check middle row
    let mid = height / 2;
    println!("\nMiddle row (row {}):", mid);
    let mut non_white_count = 0;
    for x in 0..width.min(50) {
        let idx = (mid * width + x) * 4;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        let b = rgba[idx + 2];
        if !(r == 255 && g == 255 && b == 255) {
            non_white_count += 1;
            print!("█");
        } else {
            print!(" ");
        }
    }
    println!(" ({} non-white pixels)", non_white_count);
}
