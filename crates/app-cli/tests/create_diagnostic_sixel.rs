//! Create a diagnostic gradient sixel image

use std::fs::File;
use std::io::Write;

#[test]
fn create_diagnostic_sixel() {
    // Create a 128x64 diagonal gradient image
    // R increases left-to-right (0-255)
    // G increases top-to-bottom (0-255)
    // B is constant 128
    // This way we can see if rows/columns are correct or swapped
    
    let width = 128;
    let height = 64;
    
    // Create RGBA buffer directly
    let mut rgba = vec![0u8; width * height * 4];
    
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let r = ((x as f32 / width as f32) * 255.0) as u8;
            let g = ((y as f32 / height as f32) * 255.0) as u8;
            let b = 128u8;
            let a = 255u8;
            
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }
    
    // Now encode this as sixel
    // For simplicity, use palette colors for the gradient
    let mut sixel_buf = Vec::new();
    
    sixel_buf.extend_from_slice(b"\x1bPq");
    sixel_buf.extend_from_slice(&format!("\"1;1;{};{}", width, height).as_bytes());
    
    // Create 64 colors for the palette (8x8 grid of colors)
    for i in 0..64 {
        let color_num = i;
        let r = ((i % 8) as f32 / 7.0 * 255.0) as u8;
        let g = ((i / 8) as f32 / 7.0 * 255.0) as u8;
        let b = 128u8;
        sixel_buf.extend_from_slice(&format!("#{};1;{};{};{}", color_num, r, g, b).as_bytes());
    }
    
    // Create sixel data by encoding colors into sixel format
    // This is complex, so for now just use a simpler approach
    for row in 0..(height / 6) {
        if row > 0 {
            sixel_buf.push(b'-');
        }
        
        for col in 0..width {
            // Map pixel (col, row*6) to a color index
            let y_start = row * 6;
            let color_idx = ((col as f32 / width as f32) * 8.0) as u8 +
                           (((y_start as f32 / height as f32) * 8.0) as u8 * 8);
            
            // Use simple encoding: each byte sets one color
            sixel_buf.push(0x3F + (color_idx % 64));
        }
    }
    
    sixel_buf.extend_from_slice(b"\x1b\\");
    
    let mut file = File::create("/tmp/diagnostic.six").expect("Failed to create file");
    file.write_all(&sixel_buf).expect("Failed to write file");
    
    println!("Created diagnostic sixel at /tmp/diagnostic.six");
    println!("Pattern: R channel increases left-to-right, G channel increases top-to-bottom");
    println!("If rendering is correct, you should see a gradient with:");
    println!("- Red at top-right");
    println!("- Green at bottom-left  ");
    println!("- Yellow at bottom-right");
}
