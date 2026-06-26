//! Test to debug sixel row-column layout

use terminal_core::sixel::decode_sixel;
use std::fs;

#[test]
fn test_sixel_row_structure() {
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
    
    println!("Decoded dimensions: {}x{}", width, height);
    println!("RGBA buffer size: {} bytes ({} pixels)", rgba.len(), rgba.len() / 4);
    
    // Check the actual pixel data distribution
    let mut profile_map = std::collections::HashMap::new();
    for i in (0..rgba.len()).step_by(4) {
        let r = rgba[i];
        let g = rgba[i + 1];
        let b = rgba[i + 2];
        let a = rgba[i + 3];
        let key = format!("RGBA({},{},{},{})", r, g, b, a);
        *profile_map.entry(key).or_insert(0) += 1;
    }
    
    println!("\nPixel color distribution:");
    let mut entries: Vec<_> = profile_map.iter().collect();
    entries.sort_by_key(|(_,count)| std::cmp::Reverse(**count));
    for (color, count) in entries.iter().take(20) {
        let percent = (**count as f32 / (rgba.len() / 4) as f32) * 100.0;
        println!("  {}: {} pixels ({:.1}%)", color, count, percent);
    }
    
    // Check specific rows for patterns
    println!("\nRow-by-row non-white pixel count (first 100 rows):");
    for row in 0..height.min(100) {
        let mut colored_count = 0;
        for col in 0..width {
            let idx = (row * width + col) * 4;
            if idx + 3 < rgba.len() {
                let r = rgba[idx];
                let g = rgba[idx + 1];
                let b = rgba[idx + 2];
                if !(r == 255 && g == 255 && b == 255) {
                    colored_count += 1;
                }
            }
        }
        if colored_count > 0 {
            println!("  Row {}: {} non-white pixels", row, colored_count);
        }
    }
    
    // Check column distribution
    println!("\nColumn distribution (width profile):");
    let mut col_counts = vec![0; width];
    for row in 0..height {
        for col in 0..width {
            let idx = (row * width + col) * 4;
            if idx + 3 < rgba.len() {
                let r = rgba[idx];
                let g = rgba[idx + 1];
                let b = rgba[idx + 2];
                if !(r == 255 && g == 255 && b == 255) {
                    col_counts[col] += 1;
                }
            }
        }
    }
    
    let mut empty_cols = 0;
    let mut full_cols = 0;
    for (col, count) in col_counts.iter().enumerate() {
        if *count == 0 {
            empty_cols += 1;
        } else if *count > height - 10 {
            full_cols += 1;
        }
        if col < 10 || col % 75 == 0 {
            println!("  Col {}: {} non-white pixels", col, count);
        }
    }
    println!("  Empty columns: {}, Near-full columns: {}", empty_cols, full_cols);
}
