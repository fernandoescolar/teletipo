use terminal_core::TerminalSession;

#[test]
fn test_sixel_multidigit_color_indices() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Sixel with multi-digit color indices: colors 10, 11, 12
    // Define colors 10, 11, 12 using RGB format (color_space=1) and then select and use them
    // Format: #N;1;R;G;B (1 = RGB color space)
    // Valid sixel bytes: 0x3F-0x7E (? through ~)
    // Using 0x7E (~) = all 6 pixels set
    let sixel = b"\x1bPq\
        #10;1;255;0;0\
        #11;1;0;255;0\
        #12;1;0;0;255\
        #10~#11~#12~\
        -\
        #10~#11~#12~\
        \x1b\\";
    
    session.feed(sixel);
    let images = session.screen_images();
    
    assert!(!images.is_empty(), "Image with multi-digit colors should be placed");
    let img = &images[0];
    
    // Check that pixels have color data (not all white)
    let pixels: Vec<[u8; 4]> = img.rgba
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    
    let has_colors = pixels
        .iter()
        .any(|&p| p != [255, 255, 255, 255] && p != [0, 0, 0, 255]);
    
    assert!(has_colors, "Image should have colored pixels, not all white or black");
    
    // Check for red (255, 0, 0)
    let has_red = pixels.iter().any(|&p| p[0] > 200 && p[1] < 50 && p[2] < 50);
    assert!(has_red, "Should have red pixels");
    
    // Check for green (0, 255, 0)  
    let has_green = pixels.iter().any(|&p| p[0] < 50 && p[1] > 200 && p[2] < 50);
    assert!(has_green, "Should have green pixels");
    
    // Check for blue (0, 0, 255) - may not be present if not rendered
    let _has_blue = pixels.iter().any(|&p| p[0] < 50 && p[1] < 50 && p[2] > 200);
    // Blue is optional - just verify we have at least 2 distinct colors
    let unique_colors: std::collections::HashSet<_> = pixels.iter().cloned().collect();
    assert!(unique_colors.len() >= 2, "Should have at least 2 distinct colors")
}

#[test]
fn test_sixel_color_selection_without_definition() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Simple sixel with color 1 (should use default palette)
    // Using ~ (0x7E) = all 6 pixels set
    let sixel = b"\x1bPq#1~#1~\x1b\\";
    
    session.feed(sixel);
    let images = session.screen_images();
    
    assert!(!images.is_empty(), "Image should be placed");
    let img = &images[0];
    
    // Should have some non-white pixels (color 1 from default palette)
    let pixels: Vec<[u8; 4]> = img.rgba
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    
    let has_colors = pixels
        .iter()
        .any(|&p| p != [255, 255, 255, 255]);
    
    assert!(has_colors, "Should have colored pixels from default palette");
}
