use terminal_core::TerminalSession;

#[test]
fn test_sixel_large_image_600x450() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Large image: 600x450 pixels (75 cols x 29 rows)
    // Format: "1;1;600;450 with palette and data
    let sixel = b"\x1bPq\"1;1;600;450#0;2;255;0;0#1;2;0;255;0!100-\x1b\\";
    
    session.feed(sixel);
    let images = session.screen_images();
    
    assert!(!images.is_empty(), "Image should be placed on screen");
    let img = &images[0];
    
    // Verify raster dimensions are used, not calculated from data
    assert_eq!(img.width_px, 600, "Image width should be 600 pixels (from raster)");
    assert_eq!(img.height_px, 450, "Image height should be 450 pixels (from raster)");
    
    // Verify RGBA buffer is correct size
    assert_eq!(
        img.rgba.len(),
        600 * 450 * 4,
        "RGBA buffer should be 600*450*4 bytes"
    );
}

#[test]
fn test_sixel_image_scaling() {
    let mut session = TerminalSession::new(30, 120).expect("session creation");

    // Test a 200x200 image that will be scaled
    let sixel_200 = b"\x1bPq\"1;1;200;200#0;2;100;100;100!40-\x1b\\";
    session.feed(sixel_200);
    
    let images = session.screen_images();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].width_px, 200);
    assert_eq!(images[0].height_px, 200);
}

#[test]
fn test_sixel_small_image_32x32() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Small image: 32x32 pixels (4 cols x 2 rows)
    let sixel = b"\x1bPq\"1;1;32;32#0;2;255;0;0#1;2;0;255;0!4~/!4}\x1b\\";
    
    session.feed(sixel);
    let images = session.screen_images();
    
    assert!(!images.is_empty(), "Small image should be placed");
    let img = &images[0];
    
    assert_eq!(img.width_px, 32);
    assert_eq!(img.height_px, 32);
    assert_eq!(img.rgba.len(), 32 * 32 * 4);
}

#[test]
fn test_sixel_nonsquare_image() {
    let mut session = TerminalSession::new(24, 80).expect("session creation");

    // Rectangular image: 400x100 pixels
    let sixel = b"\x1bPq\"1;1;400;100#0;2;128;128;128!50-\x1b\\";
    
    session.feed(sixel);
    let images = session.screen_images();
    
    assert!(!images.is_empty());
    let img = &images[0];
    
    assert_eq!(img.width_px, 400, "Width should be 400");
    assert_eq!(img.height_px, 100, "Height should be 100");
    assert_eq!(img.rgba.len(), 400 * 100 * 4, "RGBA size correct");
}
