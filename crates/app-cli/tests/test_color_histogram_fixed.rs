//! Test to analyze color distribution after fix

use terminal_core::sixel::decode_sixel;
use std::fs;
use std::collections::HashMap;

#[test]
fn test_color_histogram_after_fix() {
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
    
    let mut color_counts: HashMap<String, usize> = HashMap::new();
    
    for i in 0..rgba.len() / 4 {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        let b = rgba[i * 4 + 2];
        let key = format!("RGB({}, {}, {})", r, g, b);
        *color_counts.entry(key).or_insert(0) += 1;
    }
    
    let total_pixels = (width * height) as f64;
    
    println!("\nColor distribution (top 30 most common):");
    
    let mut colors: Vec<_> = color_counts.iter().collect();
    colors.sort_by(|a, b| b.1.cmp(a.1));
    
    for (color, count) in colors.iter().take(30) {
        let pct = **count as f64 / total_pixels * 100.0;
        println!("  {}: {} pixels ({:.1}%)", color, count, pct);
    }
    
    // Calculate how much of the image is "snake" (brown-ish) vs white
    let brown_pixels = color_counts.iter()
        .filter(|(k, _)| k.starts_with("RGB(") && !k.contains("255, 255, 255") && !k.contains("256, 256, 256"))
        .map(|(_, v)| v)
        .sum::<usize>();
    
    let white_pixels = *color_counts.get("RGB(255, 255, 255)").unwrap_or(&0);
    
    println!("\nSummary:");
    println!("  Total pixels: {}", (width * height));
    println!("  White pixels: {} ({:.1}%)", white_pixels, white_pixels as f64 / total_pixels * 100.0);
    println!("  Non-white pixels: {} ({:.1}%)", brown_pixels, brown_pixels as f64 / total_pixels * 100.0);
    println!("  Image dimensions: {}x{}", width, height);
}
