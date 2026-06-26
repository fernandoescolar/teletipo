use terminal_core::TerminalSession;

#[test]
fn test_sixel_image_placement() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Send a simple sixel image wrapped in DCS ... ST
    // Format: ESC P q [raster attrs] ; [palette] ; [graphics data] ESC \
    // Minimal sixel: 2x2 red pixels
    // NOTE: DCS is ESC P (0x1b 0x50), data content, and ST is ESC \ (0x1b 0x5c)
    let sixel_seq = b"\x1bPq#0;2;100;100;100#1;2;255;0;0\"2;2;0;0#0-#1--\x1b\\";
    
    session.feed(sixel_seq);

    // Get the screen images
    let images = session.screen_images();
    
    println!("Number of images on screen: {}", images.len());
    for (i, img) in images.iter().enumerate() {
        println!(
            "Image {}: id={}, pos=({},{}), size={}x{}, rgba_len={}",
            i, img.id, img.row, img.col, img.width_px, img.height_px, img.rgba.len()
        );
    }

    // Verify image was placed
    assert!(
        !images.is_empty(),
        "No images found on screen after sending sixel sequence"
    );

    // Verify image has valid dimensions
    let img = &images[0];
    assert!(img.width_px > 0, "Image width should be positive");
    assert!(img.height_px > 0, "Image height should be positive");
    assert!(
        img.rgba.len() == img.width_px * img.height_px * 4,
        "RGBA buffer size should match dimensions"
    );
}

#[test]
fn test_multiple_sixel_images() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Send two sixel images
    let sixel1 = b"\x1bPq#0;2;100;100;100#1;2;255;0;0\"2;2;0;0#0-#1--\x1b\\";
    let sixel2 = b"\x1bPq#0;2;0;255;0#1;2;0;0;255\"2;2;0;0#0--#1-\x1b\\";
    
    session.feed(sixel1);
    session.feed(b"\n");
    session.feed(sixel2);

    let images = session.screen_images();
    
    println!("Number of images on screen: {}", images.len());

    // Verify both images are present
    assert_eq!(images.len(), 2, "Should have 2 images on screen");
}

#[test]
fn test_sixel_parsing_preserves_text() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Send text, then sixel, then more text
    session.feed(b"Hello ");
    session.feed(b"\x1bPq#0;2;100;100;100\"1;1;0#0-\x1b\\");
    session.feed(b" World");

    let text = session.snapshot_text();
    let images = session.screen_images();
    
    println!("Text snapshot:\n{}", text);
    println!("Number of images: {}", images.len());

    // Verify text is still there
    assert!(text.contains("Hello"), "Text 'Hello' should be preserved");
    assert!(text.contains("World"), "Text 'World' should be preserved");

    // Verify image was placed
    assert!(!images.is_empty(), "Image should be placed between text");
}
