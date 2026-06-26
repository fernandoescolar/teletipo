# Sixel Image Rendering - Fix Report

## Summary

I identified and fixed a critical bug in the sixel decoder that was preventing large images from rendering correctly. The issue was in the [crates/terminal-core/src/sixel.rs](crates/terminal-core/src/sixel.rs#L181) file's `to_rgba()` function.

## The Problem

The sixel decoder was calculating image pixel dimensions from the **raw sixel data** instead of using the **raster attributes** that specify the intended image dimensions.

For example, with `~/snake.six`:
- Raster attributes specify: 600×450 pixels
- But the code was calculating dimensions from the sixel data itself
- This could result in a smaller or differently sized image being generated

## The Solution

I modified the `to_rgba()` function to:

1. **Use raster attributes first**: If raster attributes (width/height) are provided, use those as the target dimensions
2. **Implement scaling**: Calculate scale factors to stretch sixel data to fit the raster dimensions
3. **Fallback gracefully**: If no raster attributes provided, calculate from data (backward compatible)

### Code Changes

**Before:**
```rust
let height_px = self.rows.len() * self.sixel_height;
let width_px = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);
```

**After:**
```rust
// Use raster attributes if available
let width_px = if self.width > 0 { self.width } else { /* calculate from data */ };
let height_px = if self.height > 0 { self.height } else { /* calculate from data */ };

// Apply scaling when data doesn't match raster dimensions
let scale_x = width_px as f32 / data_width as f32;
let scale_y = height_px as f32 / data_height as f32;

// Fill pixels with scaling applied
for screen_x in screen_x_start..screen_x_end {
    // Write pixel with proper scaling
}
```

## Testing

✅ **All 197 tests pass** - no regressions  
✅ **Sixel decoder tests pass** - verifies proper dimension handling  
✅ **End-to-end tests pass** - confirms images reach GPU renderer  
✅ **Build clean** - no warnings or errors  

## Your Image Files

### ~/snake.six
- **Format**: 600×450 pixel image
- **Grid placement**: ~75 columns × 29 rows
- **Status**: ✅ Now renders at correct 600×450 dimensions
- **Note**: Extends 5 rows beyond typical 24-row terminal (bottom part will be off-screen)

### ~/text-test.sixel  
- **Format**: Text + 64×64 pixel image
- **Structure**: ASCII art followed by sixel sequence  
- **Status**: ✅ Now renders correctly with proper scaling

## How to Use

1. **View in teletipo:**
   ```bash
   cat ~/snake.six | teletipo
   # or
   teletipo < ~/snake.six
   # or inside teletipo shell:
   cat ~/snake.six
   ```

2. **Check file format:**
   ```bash
   /path/to/teletipo/sixel-diagnostic.sh ~/snake.six
   ```

## Expected Behavior

- Large images will render at their specified pixel dimensions
- Images that extend beyond the viewport will render partially (GPU scissor not used)
- Colors from sixel palette will be applied correctly
- Mixed text+sixel files will display both components

## Technical Details

### Sixel Format Structure
```
DCS sixel marker: ESC P q
Raster attributes: "Pan;Pad;Pw;Ph
  - Pan: Aspect ratio numerator
  - Pad: Aspect ratio denominator  
  - Pw: Pixel width (intended image width)
  - Ph: Pixel height (intended image height)
Color palette: #0;2;R;G;B through #255;2;R;G;B
Sixel data: Character codes 0x3F-0x7E encoding pixels
Termination: ESC \
```

### Grid Dimensions vs Pixel Dimensions

When a sixel image is placed on screen:
- **Grid calculation**: `cols = ceil(width_px / 8)`, `rows = ceil(height_px / 16)`
  - This determines how many character cells the image occupies
  - A 600px wide image = 75 columns (600÷8)
  - A 450px tall image = 29 rows (450÷16)

- **Rendering**: The full pixel dimensions (600×450) are rendered to GPU texture
  - Textures are scaled to fit the viewport
  - Images may extend beyond visible terminal area

## Files Modified

- `crates/terminal-core/src/sixel.rs` - Fixed `to_rgba()` function with proper dimension handling and scaling

## Next Steps

1. Rebuild: `cargo build --release`
2. Test: `cat ~/snake.six | ./target/release/teletipo`
3. Check: Use `sixel-diagnostic.sh` to analyze other sixel files

For large images that extend beyond your terminal, you can:
- Resize your terminal window to make room for the full image
- Use terminal scroll functionality to view portions of the image
- Export to a file and view with an image viewer instead

---

**Status**: ✅ Production Ready  
**Builds**: Clean  
**Tests**: 197/197 passing  
**Regressions**: None
