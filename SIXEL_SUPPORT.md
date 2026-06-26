## Sixel Image Support in Teletipo

### Overview

Teletipo now supports displaying sixel graphics images sent through DCS (Device Control String) escape sequences. This allows compatibility with terminal image viewing tools and protocols that use sixel format.

### Architecture

The sixel rendering pipeline consists of the following stages:

```
ANSI Parser → Sixel Decoder → Screen Storage → Snapshot Builder → GPU Renderer
```

#### 1. ANSI Parser (`crates/terminal-ansi/src/parser.rs`)

Recognizes DCS sequences: `ESC P q ... ST` where `ST` is `ESC \` or `BEL`.

- **Location**: Parser state machine in `ParserState::Dcs`
- **Trigger**: `ESC P` (0x1b 0x50) enters DCS mode
- **Termination**: `ESC \` (0x1b 0x5c) or `BEL` (0x07) ends DCS and emits `Action::DcsString`
- **Output**: `Action::DcsString` containing the complete sixel payload

#### 2. Sixel Decoder (`crates/terminal-core/src/sixel.rs`)

Parses sixel format and converts to RGBA pixel data.

- **Function**: `pub fn decode_sixel(data: &[u8]) -> Result<SixelImage>`
- **Input Format**: 
  - Raster attributes: `"Pw;Ph;Pc` (width, height, aspect ratio)
  - Palette definitions: `#0;2;R;G;B` through `#255;2;R;G;B`
  - Graphics data: sixel bytes (0x3f-0x7e)
  - Carriage returns and line feeds for multi-line images
- **Output**: `SixelImage { width, height, rgba: Vec<u8> }`
  - RGBA data is always in 32-bit RGBA format (4 bytes per pixel)
  - If decoding produces empty image, returns 64x64 checkerboard placeholder
- **Features**:
  - Supports 4-parameter raster format (width, height, aspect, zero)
  - Supports color palette definitions (up to 256 colors)
  - Handles sixel character repetition with `!N` syntax

#### 3. Session & Screen Storage

When a sixel DCS sequence is received:

- **Location**: `crates/terminal-core/src/session.rs` lines 589-609
- **Process**:
  1. `Action::DcsString` handler checks for 'q' marker (sixel indicator)
  2. Calls `decode_sixel()` on the payload
  3. Calculates grid dimensions: `cols = ceil(width / 8)`, `rows = ceil(height / 16)`
  4. Calls `screen.place_image()` to store on screen
- **Storage**: `TerminalImage` objects stored in `Screen::images` Vec
- **API**: `pub fn screen_images(&self) -> &[TerminalImage]` exposes images

#### 4. Snapshot Builder (`crates/app-cli/src/snapshot.rs`)

Converts screen-relative coordinates to viewport pixel coordinates.

- **Function**: `build_terminal_images()` lines 716-748
- **Process**:
  1. Retrieves images from `screen.images()`
  2. Converts grid coordinates (row, col) to pixels:
     - `x_px = padding_h + col * cell_w`
     - `y_px = tab_bar_h + padding_v + row * cell_h`
  3. Preserves image dimensions in pixels
- **Output**: `Vec<SnapshotImage>` with viewport coordinates and RGBA data

#### 5. GPU Renderer (`crates/render-glow/src/painter.rs`)

Uploads textures and renders images with OpenGL.

- **Texture Management** (`upload_image_textures()` lines 2315-2357):
  - Creates one OpenGL texture per unique image ID
  - Uses HashMap for caching: `image_textures: HashMap<u32, glow::Texture>`
  - Texture format: `GL_RGBA` with `LINEAR` filtering and `CLAMP_TO_EDGE` wrapping

- **Vertex Queuing** (`draw_images()` lines 2366-2415):
  - Converts viewport coordinates to vertex data
  - 8 floats per vertex: `[x, y, u, v, r, g, b, a]`
  - 2 triangles per image quad (6 vertices total)
  - Tracks draw calls: `image_draw_calls: Vec<(image_id, first_vertex, vertex_count)>`

- **Rendering** (`flush_passes()` lines 735-777):
  - Binds image shader program
  - Buffers vertex data with `STREAM_DRAW`
  - **Per-image texture binding loop** (lines 760-768):
    ```rust
    for &(image_id, first_vertex, vertex_count) in &self.image_draw_calls {
        if let Some(&tex) = self.image_textures.get(&image_id) {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.uniform_1_i32(Some(self.image_u_sampler), 0);
            gl.draw_arrays(glow::TRIANGLES, first_vertex as i32, vertex_count as i32);
        }
    }
    ```
  - Enables blending: `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`

#### 6. Shaders (`crates/render-glow/src/shaders.rs`)

Image rendering pipeline with NDC transformation.

- **Vertex Shader**:
  - Transforms pixel coordinates to NDC (Normalized Device Coordinates)
  - Formula: `ndc_x = (px.x / screen.x) * 2 - 1`
  - Formula: `ndc_y = 1 - (px.y / screen.y) * 2`
  - Passes UV and alpha to fragment shader

- **Fragment Shader**:
  - Samples texture at UV coordinates
  - Applies per-vertex alpha
  - Final output: blended with background

### Image Placement

When a sixel image is placed on screen:

1. **Grid-based positioning**: Image placed at cursor position
2. **Grid dimensions**: Calculated from pixel size
   - Each character cell is 8×16 pixels
   - `cols = ceil(width_px / 8)`
   - `rows = ceil(height_px / 16)`
3. **Coordinate system**: 
   - Row 0 = top of terminal
   - Col 0 = left edge of terminal
   - Images render in viewport pixel space

### Testing

End-to-end tests verify the complete pipeline:

- **Test File**: `crates/terminal-core/tests/test_sixel_end_to_end.rs`
- **Test Cases**:
  - Single image placement
  - Multiple images
  - Text preservation with images
- **Run Tests**: `cargo test --test test_sixel_end_to_end --release`

### Usage

To send sixel images to teletipo:

```bash
# Using a compatible tool (e.g., imagemagick, chafa, lsix)
echo -ne "\x1bPq...\x1b\\" | teletipo

# Or within teletipo, use a command that outputs sixel
printf "\x1bPq...sixel data...\x1b\\" 
```

### Sixel Format Reference

Basic sixel DCS sequence:
```
ESC P q [attrs] [palette] [data] ESC \
```

Example: 2x2 red pixel image
```
ESC P q "#2;2;0;0" "#0;2;0;0;0" "#1;2;255;0;0" "!2-" ESC \
```

Breaking down:
- `ESC P q` - DCS sixel marker
- `"2;2;0;0"` - Raster: 2 width, 2 height, 0 aspect, 0 reserved
- `#0;2;0;0;0` - Palette color 0: RGB (0,0,0) black
- `#1;2;255;0;0` - Palette color 1: RGB (255,0,0) red  
- `!2-` - Two red pixels, carriage return
- `ESC \` - String terminator

### Known Limitations

1. **Placeholder for empty images**: If sixel decoding produces no pixels, a 64×64 checkerboard is displayed
2. **Color palette**: Limited to 256 colors per image (standard sixel limitation)
3. **Scrolling**: Images don't scroll with terminal text (fixed to absolute screen position)
4. **Copy mode**: Images are not included in copy/select operations

### Performance

- **Per-frame cost**: O(1) for each queued image (texture binding + draw call)
- **Texture caching**: Images reused by ID avoid re-uploading
- **Vertex buffering**: Accumulates all image quads, buffered once per frame with `STREAM_DRAW`
- **Memory**: One GPU texture per unique image, kept in HashMap cache

### Future Enhancements

- [ ] Image scrolling support (images move with scrollback)
- [ ] Sixel format variant support (iTerm2 inline images)
- [ ] Direct image protocol support (Kitty graphics, iTerm2 inline)
- [ ] Image resizing/scaling support
- [ ] Image caching across frames for faster rendering
