use terminal_core::decode_sixel;
use std::fs;

fn main() {
    let data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Find DCS payload (skip ESC P q and final ESC \)
    let payload_start = 3; // After ESC P q
    let payload_end = data.len() - 2; // Before ESC \
    let payload = &data[payload_start..payload_end];
    
    println!("File size: {} bytes", data.len());
    println!("Payload size: {} bytes", payload.len());
    
    // Sample the first color definitions and sixel data
    let payload_str = String::from_utf8_lossy(payload);
    let lines: Vec<&str> = payload_str.lines().collect();
    
    println!("\nFirst color definitions (first 10 lines):");
    for (i, line) in lines.iter().take(10).enumerate() {
        println!("  {}: {}", i, &line[..line.len().min(100)]);
    }
    
    // Look for actual sixel data (bytes 0x3F-0x7E)
    let mut sixel_count = 0;
    let mut last_sixel_pos = 0;
    for (i, &b) in payload.iter().enumerate() {
        if b >= 0x3F && b <= 0x7E {
            sixel_count += 1;
            last_sixel_pos = i;
        }
    }
    
    println!("\nSixel data bytes found: {}", sixel_count);
    println!("Last sixel byte at position: {}", last_sixel_pos);
    
    // Try to decode
    match decode_sixel(payload) {
        Ok(img) => {
            println!("\nDecoded successfully!");
            println!("Image size: {}x{}", img.width, img.height);
            println!("RGBA buffer size: {} bytes", img.rgba.len());
            
            // Count colors
            let mut color_counts: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();
            for chunk in img.rgba.chunks_exact(4) {
                let color = [chunk[0], chunk[1], chunk[2], chunk[3]];
                *color_counts.entry(color).or_insert(0) += 1;
            }
            
            println!("Total unique colors: {}", color_counts.len());
            
            let mut colors_vec: Vec<_> = color_counts.iter().collect();
            colors_vec.sort_by_key(|&(_, count)| std::cmp::Reverse(*count));
            
            println!("Top 10 colors:");
            for (i, (color, count)) in colors_vec.iter().take(10).enumerate() {
                println!("  {}: RGBA({},{},{},{}): {} pixels", 
                    i, color[0], color[1], color[2], color[3], count);
            }
        }
        Err(e) => {
            println!("Decode error: {}", e);
        }
    }
}
