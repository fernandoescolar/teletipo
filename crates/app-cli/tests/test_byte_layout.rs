//! Test to examine byte layout and specific pixel colors

use terminal_core::sixel::decode_sixel;
use std::fs;

#[test]
fn test_sixel_byte_layout() {
    let sixel_data = fs::read("/home/fernando/snake.six").expect("Failed to read snake.six");
    
    let payload = if sixel_data.len() > 3 && sixel_data[0] == 0x1b && sixel_data[1] == b'P' && sixel_data[2] == b'q' {
        &sixel_data[3..]
    } else {
        &sixel_data
    };
    
    let payload = if payload.len() >= 2 && payload[payload.len()-2] == 0x1b && payload[payload.len()-1] == b'\\' {
        &payload[..payload.len()-2]
    } else {
        payload
    };
    
    let sixel_img = decode_sixel(payload).expect("Failed to decode sixel");
    let (rgba, width, height) = (sixel_img.rgba, sixel_img.width as usize, sixel_img.height as usize);
    
    println!("Image: {}x{}", width, height);
    println!("RGBA buffer size: {} bytes", rgba.len());
    
    // Check first few pixels (should be at top-left)
    println!("\nFirst 10 pixels (top-left corner):");
    for i in 0..10.min(width) {
        let idx = i * 4;
        println!(
            "  Pixel {}: R={} G={} B={} A={}",
            i,
            rgba[idx],
            rgba[idx + 1],
            rgba[idx + 2],
            rgba[idx + 3]
        );
    }
    
    // Check last pixel of first row
    println!("\nLast pixel of first row (top-right corner area):");
    let last_pixel_row0 = (width - 1) * 4;
    println!(
        "  R={} G={} B={} A={}",
        rgba[last_pixel_row0],
        rgba[last_pixel_row0 + 1],
        rgba[last_pixel_row0 + 2],
        rgba[last_pixel_row0 + 3]
    );
    
    // Check first pixel of second row
    println!("\nFirst pixel of second row:");
    let first_pixel_row1 = width * 4;
    println!(
        "  R={} G={} B={} A={}",
        rgba[first_pixel_row1],
        rgba[first_pixel_row1 + 1],
        rgba[first_pixel_row1 + 2],
        rgba[first_pixel_row1 + 3]
    );
    
    // Check first pixel of last row
    println!("\nFirst pixel of last row (bottom-left):");
    let first_pixel_last_row = (height - 1) * width * 4;
    println!(
        "  R={} G={} B={} A={}",
        rgba[first_pixel_last_row],
        rgba[first_pixel_last_row + 1],
        rgba[first_pixel_last_row + 2],
        rgba[first_pixel_last_row + 3]
    );
    
    // Check last pixel of last row (bottom-right)
    println!("\nLast pixel of last row (bottom-right):");
    let last_pixel_last_row = (height * width - 1) * 4;
    println!(
        "  R={} G={} B={} A={}",
        rgba[last_pixel_last_row],
        rgba[last_pixel_last_row + 1],
        rgba[last_pixel_last_row + 2],
        rgba[last_pixel_last_row + 3]
    );
    
    // Count brown pixels (should be most of the image)
    let mut brown_count = 0;
    let mut white_count = 0;
    for i in (0..rgba.len()).step_by(4) {
        let r = rgba[i];
        let g = rgba[i + 1];
        let b = rgba[i + 2];
        // Brown: RGB(96,83,61)
        if r > 90 && r < 102 && g > 78 && g < 88 && b > 56 && b < 66 {
            brown_count += 1;
        }
        // White: RGB(255,255,255)
        if r == 255 && g == 255 && b == 255 {
            white_count += 1;
        }
    }
    
    println!(
        "\nStatistics: Brown pixels: {}, White pixels: {}",
        brown_count, white_count
    );
}
