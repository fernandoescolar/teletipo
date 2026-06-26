//! Debug test to check palette and colors

use std::fs;
use terminal_core::sixel::decode_sixel;

#[test]
fn test_palette_debug() {
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
    
    // Decode the image
    let result = decode_sixel(payload).expect("Failed to decode sixel");
    println!("Decoded image: {}x{}", result.width, result.height);
    
    // Check color distribution
    let mut color_counts = std::collections::HashMap::new();
    for chunk in result.rgba.chunks(4) {
        if chunk.len() >= 4 {
            let color = (chunk[0], chunk[1], chunk[2]);
            *color_counts.entry(color).or_insert(0) += 1;
        }
    }
    
    println!("\nTop 20 colors by frequency:");
    let mut colors: Vec<_> = color_counts.iter().collect();
    colors.sort_by(|a, b| b.1.cmp(a.1));
    
    for ((r, g, b), count) in colors.iter().take(20) {
        let pct = (**count as f32 / (result.width * result.height) as f32) * 100.0;
        if *r == 255 && *g == 255 && *b == 255 {
            println!("  WHITE: {} pixels ({:.1}%)", count, pct);
        } else if *r == 0 && *g == 0 && *b == 0 {
            println!("  BLACK: {} pixels ({:.1}%)", count, pct);
        } else {
            println!("  RGB({:3}, {:3}, {:3}): {} pixels ({:.1}%)", r, g, b, count, pct);
        }
    }
    
    // Check if image is mostly white or has good color distribution
    let white_count = color_counts.get(&(255, 255, 255)).copied().unwrap_or(0);
    let total_pixels = result.width * result.height;
    let white_pct = (white_count as f32 / total_pixels as f32) * 100.0;
    println!("\nOverall: {:.1}% white, {:.1}% colored", white_pct, 100.0 - white_pct);
}
