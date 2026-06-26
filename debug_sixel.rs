use terminal_core::decode_sixel;

fn main() {
    // Simple test: color 1 defined as red, then draw with it
    let sixel = b"\x1bPq#1;2;255;0;0#1?>\x1b\\";
    
    println!("Testing sixel decoding with simple example...");
    println!("Sixel bytes: {:?}", std::str::from_utf8(sixel).unwrap_or("<invalid utf8>"));
    
    match decode_sixel(&sixel[2..]) {
        Ok(img) => {
            println!("Decoded successfully!");
            println!("Image dimensions: {}x{}", img.width, img.height);
            println!("RGBA buffer size: {} bytes", img.rgba.len());
            
            // Check pixel data
            let mut colors = std::collections::HashMap::new();
            for chunk in img.rgba.chunks_exact(4) {
                let color = format!("[{},{},{},{}]", chunk[0], chunk[1], chunk[2], chunk[3]);
                *colors.entry(color).or_insert(0usize) += 1;
            }
            
            println!("Unique colors in image:");
            for (color, count) in &colors {
                println!("  {}: {} pixels", color, count);
            }
        }
        Err(e) => {
            println!("Failed to decode: {:?}", e);
        }
    }
}
