use std::fs;
use std::collections::HashMap;

#[test]
fn test_sixel_content_analysis() {
    // Read snake.six and decode it
    let data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Extract DCS payload - skip ESC P q at start
    let start = data.iter().position(|&b| b == b'P').unwrap_or(0) + 2; // Skip "Pq"
    let end = data.iter().rposition(|&b| b == b'\\').unwrap_or(data.len());
    let payload = &data[start..end];
    
    // Decode sixel
    let sixel_data = &payload; // Already past 'Pq'
    match terminal_core::sixel::decode_sixel(sixel_data) {
        Ok(sixel_image) => {
            println!("=== Sixel Content Analysis ===");
            println!("Decoded dimensions: {}x{} pixels", sixel_image.width, sixel_image.height);
            println!("Total pixels: {}", sixel_image.width * sixel_image.height);
            println!("RGBA buffer: {} bytes", sixel_image.rgba.len());
            
            // Analyze the content per row
            let width = sixel_image.width;
            let bytes_per_row = width * 4;
            
            println!("\nRow-by-row analysis (showing first 50 rows):");
            for row in 0..50.min(sixel_image.height) {
                let row_start = row * bytes_per_row;
                let row_end = row_start + bytes_per_row;
                if row_end > sixel_image.rgba.len() {
                    break;
                }
                
                let row_data = &sixel_image.rgba[row_start..row_end];
                
                // Count unique colors in this row
                let mut color_map: HashMap<[u8; 4], usize> = HashMap::new();
                for chunk in row_data.chunks_exact(4) {
                    let color = [chunk[0], chunk[1], chunk[2], chunk[3]];
                    *color_map.entry(color).or_insert(0) += 1;
                }
                
                let white_count = color_map.get(&[255, 255, 255, 255]).copied().unwrap_or(0);
                let colored_count = width - white_count;
                
                println!("Row {:3}: White: {:4} pixels, Colored: {:4} pixels, Colors in row: {}", 
                    row, white_count, colored_count, color_map.len());
                
                if colored_count > 0 {
                    println!("        Colors: {:?}", color_map.iter().take(5).collect::<Vec<_>>());
                }
            }
            
            println!("\nChecking last few rows too:");
            for row in (sixel_image.height.saturating_sub(5))..sixel_image.height {
                let row_start = row * bytes_per_row;
                let row_end = row_start + bytes_per_row;
                if row_end > sixel_image.rgba.len() {
                    continue;
                }
                
                let row_data = &sixel_image.rgba[row_start..row_end];
                let mut color_map: HashMap<[u8; 4], usize> = HashMap::new();
                for chunk in row_data.chunks_exact(4) {
                    let color = [chunk[0], chunk[1], chunk[2], chunk[3]];
                    *color_map.entry(color).or_insert(0) += 1;
                }
                
                let white_count = color_map.get(&[255, 255, 255, 255]).copied().unwrap_or(0);
                let colored_count = width - white_count;
                
                println!("Row {:3}: White: {:4} pixels, Colored: {:4} pixels", 
                    row, white_count, colored_count);
            }
        }
        Err(e) => {
            eprintln!("Failed to decode sixel: {}", e);
        }
    }
}
