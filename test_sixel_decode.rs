use std::fs;

fn main() {
    // Read the sixel file
    let data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Find the DCS sequence (ESC P q ... ESC \)
    let dcs_start = 2; // After ESC P
    let dcs_end = data.len() - 2; // Before ESC \
    
    let payload = &data[dcs_start..dcs_end];
    
    println!("Total payload bytes: {}", payload.len());
    
    // Count how many sixel bytes (0x3F-0x7E) are in the file
    let sixel_bytes: Vec<u8> = payload
        .iter()
        .filter(|&&b| b >= 0x3F && b <= 0x7E)
        .copied()
        .collect();
    
    println!("Sixel data bytes found: {}", sixel_bytes.len());
    
    // Count color definitions
    let color_defs = payload.iter().filter(|&&b| b == b'#').count();
    println!("Color definitions (#): {}", color_defs);
    
    // Count line feeds (-)
    let line_feeds = payload.iter().filter(|&&b| b == b'-').count();
    println!("Line feeds (-): {}", line_feeds);
    
    // Print first few sixel bytes
    println!("First 20 sixel bytes: {:?}", &sixel_bytes[..20.min(sixel_bytes.len())]);
    
    // Check for colors with double digits
    let payload_str = String::from_utf8_lossy(payload);
    let double_digit_colors = payload_str.matches("#10").count()
        + payload_str.matches("#11").count()
        + payload_str.matches("#12").count()
        + payload_str.matches("#99").count();
    
    println!("Double-digit color references found: {}", double_digit_colors);
}
