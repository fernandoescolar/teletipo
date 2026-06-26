//! End-to-end test to verify image rendering flow

use terminal_core::TerminalSession;

#[test]
fn test_image_rendering_flow() {
    // Create a terminal session
    let mut session = TerminalSession::new(80, 24).expect("Failed to create session");
    
    // Load and decode a sixel image
    let sixel_data = std::fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    println!("Sixel file size: {} bytes", sixel_data.len());
    println!("First 20 bytes: {:?}", &sixel_data[..20.min(sixel_data.len())]);
    
    // Feed the ENTIRE sixel sequence to the parser (including ESC P q)
    // The parser will extract the payload and send it to the session
    session.feed(&sixel_data);
    
    // Get images from session
    let images = session.screen_images();
    
    println!("Images placed on screen: {}", images.len());
    
    if !images.is_empty() {
        for (idx, img) in images.iter().enumerate() {
            println!(
                "Image {}: id={}, pos=({},{}), size={}x{}, rgba_len={}, width_px={}, height_px={}",
                idx, img.id, img.col, img.row, img.cols, img.rows, img.rgba.len(), img.width_px, img.height_px
            );
            
            // Verify RGBA data
            if img.rgba.len() > 0 {
                // Check first pixel
                let r = img.rgba[0];
                let g = img.rgba[1];
                let b = img.rgba[2];
                let a = img.rgba[3];
                println!(
                    "  First pixel: RGBA({},{},{},{})",
                    r, g, b, a
                );
                
                // Check for diversity in colors
                let mut colors = std::collections::HashSet::new();
                for i in (0..img.rgba.len()).step_by(4) {
                    let color_key = format!("RGBA({},{},{},{})", img.rgba[i], img.rgba[i+1], img.rgba[i+2], img.rgba[i+3]);
                    colors.insert(color_key);
                }
                println!("  Unique colors: {}", colors.len());
            }
        }
    } else {
        println!("ERROR: No images placed!");
    }
}
