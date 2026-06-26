use terminal_core::TerminalSession;

fn main() {
    // Test with a simple sixel: define color 1 as red, then draw
    let sixel = b"\x1bPq#1;2;255;0;0#1~\x1b\\";
    
    println!("Testing sixel: {:?}", std::str::from_utf8(sixel).unwrap_or("<invalid>"));
    
    let mut session = TerminalSession::new(24, 80).expect("session creation");
    session.feed(sixel);
    
    let images = session.screen_images();
    
    if images.is_empty() {
        println!("ERROR: No images decoded!");
        return;
    }
    
    let img = &images[0];
    println!("Image decoded: {}x{} = {} bytes", img.width, img.height, img.rgba.len());
    
    // Analyze pixels
    let mut color_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for chunk in img.rgba.chunks_exact(4) {
        let key = format!("[{:3},{:3},{:3},{:3}]", chunk[0], chunk[1], chunk[2], chunk[3]);
        *color_counts.entry(key).or_insert(0) += 1;
    }
    
    println!("\nPixel colors found:");
    let mut colors: Vec<_> = color_counts.iter().collect();
    colors.sort_by_key(|&(_, count)| std::cmp::Reverse(*count));
    for (color, count) in colors.iter().take(10) {
        println!("  {} -> {} pixels", color, count);
    }
}
