//! Test to check if palette is being populated correctly

use std::fs;

#[test]
fn test_palette_population() {
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
    
    // Parse manually to extract color definitions
    let s = String::from_utf8_lossy(payload);
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut color_defs = std::collections::HashMap::new();
    
    while i < bytes.len() {
        if bytes[i] == b'#' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            
            if i < bytes.len() && bytes[i] == b';' {
                // Color definition
                while i < bytes.len() && bytes[i] != b'#' && bytes[i] < 0x3F {
                    i += 1;
                }
                let def_str = String::from_utf8_lossy(&bytes[start..i]);
                let parts: Vec<&str> = def_str.split(';').collect();
                if parts.len() >= 2 {
                    if let Ok(idx) = parts[0].parse::<u8>() {
                        color_defs.insert(idx, def_str.to_string());
                    }
                }
            } else {
                // Skip color selection
            }
        } else {
            i += 1;
        }
    }
    
    println!("Found {} color definitions", color_defs.len());
    
    // Show first 10 color definitions
    let mut indices: Vec<_> = color_defs.keys().copied().collect();
    indices.sort();
    
    println!("\nFirst 10 color definitions:");
    for idx in indices.iter().take(10) {
        println!("  Color {}: {}", idx, color_defs[idx]);
    }
    
    // Check if color 0 is defined
    if let Some(def) = color_defs.get(&0) {
        println!("\nColor 0 is defined as: {}", def);
    } else {
        println!("\nColor 0 is NOT defined (will use default palette)");
    }
    
    // Check highest color index
    if let Some(&max_idx) = indices.iter().max() {
        println!("Highest color index: {}", max_idx);
    }
}
