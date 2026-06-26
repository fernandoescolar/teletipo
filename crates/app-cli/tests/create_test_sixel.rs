//! Create a simple test sixel image with gradient pattern

use std::fs::File;
use std::io::Write;

#[test]
fn create_test_gradient_sixel() {
    // Create a simple 256x64 gradient image for testing
    // Each row will have a different shade to verify row ordering
    let width = 256;
    let height = 64;
    let mut sixel_buf = Vec::new();
    
    // Add DCS prefix
    sixel_buf.extend_from_slice(b"\x1bPq");
    
    // Raster attributes: 1;1;256;64
    sixel_buf.extend_from_slice(b"\"1;1;256;64");
    
    // Create color palette - 64 shades of gray
    for i in 0..64 {
        let level = (i * 255 / 63) as u8;
        let def = format!("#{};1;{};{};{}", i, level, level, level);
        sixel_buf.extend_from_slice(def.as_bytes());
    }
    
    // Create sixel data - each row is solid color, cycling through palette
    for row in 0..(height / 6) {
        if row > 0 {
            sixel_buf.push(b'-'); // Line feed
        }
        
        // For each sixel row (6 pixels tall)
        let color_idx = (row * 6) % 64;
        
        // Generate one sixel byte per column
        // Bit pattern 0x3F (111111) means all 6 pixels in this column are set
        for _col in 0..width {
            sixel_buf.push(0x3F + color_idx as u8); // Sixel byte with color index
        }
        
        if row == 0 {
            // Mark color selection
            sixel_buf.extend_from_slice(&format!("#{}", color_idx).as_bytes());
        }
    }
    
    // ST terminator
    sixel_buf.extend_from_slice(b"\x1b\\");
    
    // Write to file
    let mut file = File::create("/tmp/test_gradient.six").expect("Failed to create test file");
    file.write_all(&sixel_buf).expect("Failed to write test file");
    
    println!("Created test sixel at /tmp/test_gradient.six ({}b)", sixel_buf.len());
}
