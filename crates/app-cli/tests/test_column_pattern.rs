//! Test to check if pixel columns need horizontal flipping

use terminal_core::sixel::decode_sixel;
use std::fs;
use std::collections::HashSet;

#[test]
fn test_pixel_column_pattern() {
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
    
    println!("Checking column pattern (left vs right):");
    println!();
    
    // Check leftmost columns (first 10)
    println!("Leftmost 10 columns:");
    for col in 0..10 {
        let mut colored_pixels = 0;
        for row in 0..height {
            let idx = (row * width + col) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            if !(r == 255 && g == 255 && b == 255) {
                colored_pixels += 1;
            }
        }
        println!("  Col {}: {} non-white pixels", col, colored_pixels);
    }
    
    println!();
    println!("Rightmost 10 columns:");
    for col in (width - 10)..width {
        let mut colored_pixels = 0;
        for row in 0..height {
            let idx = (row * width + col) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            if !(r == 255 && g == 255 && b == 255) {
                colored_pixels += 1;
            }
        }
        println!("  Col {}: {} non-white pixels", col, colored_pixels);
    }
    
    // Sample a row to visualize
    println!();
    println!("Row 225 visualization (50 char width):");
    for col in 0..50 {
        let idx = (225 * width + col) * 4;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        let b = rgba[idx + 2];
        if !(r == 255 && g == 255 && b == 255) {
            print!("█");
        } else {
            print!(" ");
        }
    }
    println!(" (left)");
    
    for col in (width - 50)..width {
        let idx = (225 * width + col) * 4;
        let r = rgba[idx];
        let g = rgba[idx + 1];
        let b = rgba[idx + 2];
        if !(r == 255 && g == 255 && b == 255) {
            print!("█");
        } else {
            print!(" ");
        }
    }
    println!(" (right)");
}
