use terminal_core::TerminalSession;

#[test]
fn debug_sixel_parsing() {
    // Very simple test case
    let mut session = TerminalSession::new(24, 80).expect("session creation");
    
    // Sixel: Define color 1 as red using RGB (color_space=1)
    // Format: #N;1;R;G;B (1 = RGB color space)
    let sixel_data = b"\x1bPq#1;1;255;0;0#1~\x1b\\";
    println!("\nDebug: Feeding sixel: {:?}", std::str::from_utf8(sixel_data).unwrap());
    
    session.feed(sixel_data);
    let images = session.screen_images();
    
    if images.is_empty() {
        println!("ERROR: No image decoded!");
        return;
    }
    
    let img = &images[0];
    println!("Image size: {}x{}", img.width_px, img.height_px);
    println!("RGBA buffer size: {} bytes ({} pixels)", img.rgba.len(), img.rgba.len() / 4);
    
    // Count colors
    let mut colors: std::collections::BTreeMap<[u8; 4], usize> = std::collections::BTreeMap::new();
    for chunk in img.rgba.chunks_exact(4) {
        let color = [chunk[0], chunk[1], chunk[2], chunk[3]];
        *colors.entry(color).or_insert(0) += 1;
    }
    
    println!("Colors in image:");
    for (color, count) in colors.iter() {
        println!("  RGBA({},{},{},{}): {} pixels", color[0], color[1], color[2], color[3], count);
    }
    
    // Check specific colors
    let has_red = colors.contains_key(&[255, 0, 0, 255]);
    let has_white = colors.contains_key(&[255, 255, 255, 255]);
    
    println!("Has red pixels: {}", has_red);
    println!("Has white pixels: {}", has_white);
    
    if !has_red {
        println!("\nWARNING: Color was defined as red but not found in image!");
        println!("This suggests the sixel decoder is not processing the data correctly.");
    }
}
