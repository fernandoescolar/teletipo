//! Test to verify pixel coverage - check if pixels are being written to all rows/cols

use terminal_core::sixel::decode_sixel;
use std::fs;
use std::collections::HashSet;

#[test]
fn test_pixel_coverage() {
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
    
    // Find non-white pixels to verify coverage
    let mut occupied_rows: HashSet<usize> = HashSet::new();
    let mut occupied_cols: HashSet<usize> = HashSet::new();
    
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            
            // Skip white pixels
            if !(r == 255 && g == 255 && b == 255) {
                occupied_rows.insert(y);
                occupied_cols.insert(x);
            }
        }
    }
    
    println!("Image dimensions: {}x{}", width, height);
    println!("Non-white pixel coverage:");
    println!("  Rows with pixels: {} / {} ({:.1}%)", 
        occupied_rows.len(), 
        height,
        occupied_rows.len() as f64 / height as f64 * 100.0);
    println!("  Cols with pixels: {} / {} ({:.1}%)", 
        occupied_cols.len(), 
        width,
        occupied_cols.len() as f64 / width as f64 * 100.0);
    
    if !occupied_rows.is_empty() {
        let min_row = *occupied_rows.iter().min().unwrap();
        let max_row = *occupied_rows.iter().max().unwrap();
        println!("  Row range: {} - {}", min_row, max_row);
    }
    
    if !occupied_cols.is_empty() {
        let min_col = *occupied_cols.iter().min().unwrap();
        let max_col = *occupied_cols.iter().max().unwrap();
        println!("  Col range: {} - {}", min_col, max_col);
    }
    
    // Sample some rows to see pattern
    println!("\nSample rows with content:");
    for row in occupied_rows.iter().take(10) {
        let mut col_count = 0;
        for x in 0..width {
            let idx = (row * width + x) * 4;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            if !(r == 255 && g == 255 && b == 255) {
                col_count += 1;
            }
        }
        println!("  Row {}: {} non-white pixels", row, col_count);
    }
}
