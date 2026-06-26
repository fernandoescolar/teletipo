use terminal_core::TerminalSession;
use std::fs;

#[test]
fn test_decode_real_snake_six() {
    let data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    // Create session and feed the file
    let mut session = TerminalSession::new(24, 80).expect("session creation");
    session.feed(&data);
    
    let images = session.screen_images();
    
    println!("\n=== Real snake.six Analysis ===");
    println!("File size: {} bytes", data.len());
    println!("Images decoded: {}", images.len());
    
    if images.is_empty() {
        println!("ERROR: No images decoded!");
        return;
    }
    
    let img = &images[0];
    println!("Image size: {}x{} pixels", img.width_px, img.height_px);
    println!("Grid placement: {}x{} chars (row {},  col {})", img.cols, img.rows, img.row, img.col);
    println!("RGBA buffer size: {} bytes ({} pixels)", img.rgba.len(), img.rgba.len() / 4);
    
    // Analyze pixels
    let mut color_map: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();
    for chunk in img.rgba.chunks_exact(4) {
        let color = [chunk[0], chunk[1], chunk[2], chunk[3]];
        *color_map.entry(color).or_insert(0) += 1;
    }
    
    println!("\nUnique colors: {}", color_map.len());
    let mut colors_vec: Vec<_> = color_map.iter().collect();
    colors_vec.sort_by_key(|&(_, count)| std::cmp::Reverse(*count));
    
    println!("Top 15 colors:");
    for (i, (color, count)) in colors_vec.iter().take(15).enumerate() {
        let percent = (*count * 100) / (img.rgba.len() / 4);
        println!("  {}: RGBA({:3},{:3},{:3},{:3}): {:6} pixels ({:3}%)", 
            i, color[0], color[1], color[2], color[3], count, percent);
    }
    
    // Check if we have any non-white pixels
    let white_count = color_map.get(&[255, 255, 255, 255]).copied().unwrap_or(0);
    let total_pixels = img.rgba.len() / 4;
    let colored_pixels = total_pixels - white_count;
    
    println!("\nSummary:");
    println!("  White pixels: {} ({:.1}%)", white_count, (white_count as f32 / total_pixels as f32) * 100.0);
    println!("  Colored pixels: {} ({:.1}%)", colored_pixels, (colored_pixels as f32 / total_pixels as f32) * 100.0);
}
