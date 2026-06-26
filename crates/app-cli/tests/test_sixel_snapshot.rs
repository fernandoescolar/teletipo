use std::fs;
use std::sync::Arc;

#[test]
fn test_sixel_snapshot_data_flow() {
    // Read snake.six and decode it
    let data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Extract DCS payload
    let start = data.iter().position(|&b| b == b'P').unwrap_or(0) + 2; // Skip "Pq"
    let end = data.iter().rposition(|&b| b == b'\\').unwrap_or(data.len());
    let payload = &data[start..end];
    
    println!("Payload size: {} bytes", payload.len());
    
    // Decode sixel
    // The session.rs code does payload[1..] to skip 'q',but the entire sixel data follows
    let sixel_data = &payload; // Don't skip - the payload is already past 'Pq'
    match terminal_core::sixel::decode_sixel(sixel_data) {
        Ok(sixel_image) => {
            println!("Decoded image: {}x{} pixels", sixel_image.width, sixel_image.height);
            println!("RGBA buffer: {} bytes", sixel_image.rgba.len());
            
            if sixel_image.rgba.len() >= 16 {
                println!("First 4 pixels from decoded buffer:");
                for i in 0..4 {
                    let p = &sixel_image.rgba[i * 4..(i + 1) * 4];
                    println!("  Pixel {}: RGBA({},{},{},{})", i, p[0], p[1], p[2], p[3]);
                }
            }
            
            // Now create a TerminalImage-like structure and check the data
            println!("\nAs Arc<Vec<u8>>:");
            let arc_rgba = Arc::new(sixel_image.rgba.clone());
            
            if arc_rgba.len() >= 16 {
                println!("First 4 pixels from Arc'd buffer:");
                for i in 0..4 {
                    let p = &arc_rgba[i * 4..(i + 1) * 4];
                    println!("  Pixel {}: RGBA({},{},{},{})", i, p[0], p[1], p[2], p[3]);
                }
            }
            
            println!("\nAfter clone():");
            let cloned = arc_rgba.clone();
            if cloned.len() >= 16 {
                println!("First 4 pixels from cloned Arc:");
                for i in 0..4 {
                    let p = &cloned[i * 4..(i + 1) * 4];
                    println!("  Pixel {}: RGBA({},{},{},{})", i, p[0], p[1], p[2], p[3]);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to decode sixel: {}", e);
        }
    }
}
