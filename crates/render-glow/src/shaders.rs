use glow::HasContext;

type Result<T> = anyhow::Result<T>;

/// Compile the flat-colour vertex + fragment shader program.
/// Vertex layout: `[x(2), y, r(4), g, b, a]` = 6 f32 per vertex.
pub(crate) fn compile_program(gl: &glow::Context) -> Result<glow::Program> {
    let vs_src = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec4 a_color;
uniform vec2 u_screen;
out vec4 v_color;

void main() {
    vec2 ndc = vec2((a_pos.x / u_screen.x) * 2.0 - 1.0,
                    1.0 - (a_pos.y / u_screen.y) * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_color = a_color;
}
"#;

    let fs_src = r#"#version 330 core
in vec4 v_color;
out vec4 FragColor;

void main() {
    FragColor = v_color;
}
"#;

    let program =
        unsafe { gl.create_program() }.map_err(|err| anyhow::anyhow!("create program: {err}"))?;
    let vs = unsafe { gl.create_shader(glow::VERTEX_SHADER) }
        .map_err(|err| anyhow::anyhow!("create vertex shader: {err}"))?;
    let fs = unsafe { gl.create_shader(glow::FRAGMENT_SHADER) }
        .map_err(|err| anyhow::anyhow!("create fragment shader: {err}"))?;

    unsafe {
        gl.shader_source(vs, vs_src);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("vertex shader compile error: {log}");
        }

        gl.shader_source(fs, fs_src);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("fragment shader compile error: {log}");
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            anyhow::bail!("program link error: {log}");
        }
    }

    Ok(program)
}

/// Compile the atlas-textured glyph shader program.
/// Vertex layout: `[x(2), y, u(2), v, r(4), g, b, a]` = 8 f32 per vertex.
pub(crate) fn compile_atlas_program(gl: &glow::Context) -> Result<glow::Program> {
    let vs_src = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
uniform vec2 u_screen;
out vec2 v_uv;
out vec4 v_color;

void main() {
    vec2 ndc = vec2((a_pos.x / u_screen.x) * 2.0 - 1.0,
                    1.0 - (a_pos.y / u_screen.y) * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_uv   = a_uv;
    v_color = a_color;
}
"#;

    let fs_src = r#"#version 330 core
uniform sampler2D u_atlas;
in vec2 v_uv;
in vec4 v_color;
out vec4 FragColor;

void main() {
    float coverage = texture(u_atlas, v_uv).r;
    FragColor = vec4(v_color.rgb, v_color.a * coverage);
}
"#;

    let program = unsafe { gl.create_program() }
        .map_err(|err| anyhow::anyhow!("create atlas program: {err}"))?;
    let vs = unsafe { gl.create_shader(glow::VERTEX_SHADER) }
        .map_err(|err| anyhow::anyhow!("create atlas vertex shader: {err}"))?;
    let fs = unsafe { gl.create_shader(glow::FRAGMENT_SHADER) }
        .map_err(|err| anyhow::anyhow!("create atlas fragment shader: {err}"))?;

    unsafe {
        gl.shader_source(vs, vs_src);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("atlas vertex shader compile error: {log}");
        }

        gl.shader_source(fs, fs_src);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("atlas fragment shader compile error: {log}");
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            anyhow::bail!("atlas program link error: {log}");
        }
    }

    Ok(program)
}

/// Compile the color-emoji atlas shader program.
/// Same vertex layout as the grayscale atlas (8 f32/vertex), but the fragment
/// shader samples all four RGBA channels from the texture directly, making it
/// suitable for SBIX / CBDT color emoji bitmaps.
pub(crate) fn compile_color_atlas_program(gl: &glow::Context) -> Result<glow::Program> {
    let vs_src = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
uniform vec2 u_screen;
out vec2 v_uv;
out float v_alpha;

void main() {
    vec2 ndc = vec2((a_pos.x / u_screen.x) * 2.0 - 1.0,
                    1.0 - (a_pos.y / u_screen.y) * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_uv    = a_uv;
    v_alpha = a_color.a;
}
"#;

    let fs_src = r#"#version 330 core
uniform sampler2D u_atlas;
in vec2 v_uv;
in float v_alpha;
out vec4 FragColor;

void main() {
    vec4 texel = texture(u_atlas, v_uv);
    FragColor = vec4(texel.rgb, texel.a * v_alpha);
}
"#;

    let program = unsafe { gl.create_program() }
        .map_err(|err| anyhow::anyhow!("create color atlas program: {err}"))?;
    let vs = unsafe { gl.create_shader(glow::VERTEX_SHADER) }
        .map_err(|err| anyhow::anyhow!("create color atlas vertex shader: {err}"))?;
    let fs = unsafe { gl.create_shader(glow::FRAGMENT_SHADER) }
        .map_err(|err| anyhow::anyhow!("create color atlas fragment shader: {err}"))?;

    unsafe {
        gl.shader_source(vs, vs_src);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("color atlas vertex shader compile error: {log}");
        }

        gl.shader_source(fs, fs_src);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("color atlas fragment shader compile error: {log}");
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            anyhow::bail!("color atlas program link error: {log}");
        }
    }

    Ok(program)
}

/// Compile the sixel/image shader program for direct RGBA rendering.
/// Vertex layout: `[x(2), y, u(2), v, r(4), g, b, a]` = 8 f32 per vertex.
pub(crate) fn compile_image_program(gl: &glow::Context) -> Result<glow::Program> {
    let vs_src = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
uniform vec2 u_screen;
out vec2 v_uv;
out float v_alpha;

void main() {
    vec2 ndc = vec2((a_pos.x / u_screen.x) * 2.0 - 1.0,
                    1.0 - (a_pos.y / u_screen.y) * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_uv    = a_uv;
    v_alpha = a_color.a;
}
"#;

    let fs_src = r#"#version 330 core
uniform sampler2D u_image;
in vec2 v_uv;
in float v_alpha;
out vec4 FragColor;

void main() {
    vec4 texel = texture(u_image, v_uv);
    FragColor = vec4(texel.rgb, texel.a * v_alpha);
}
"#;

    let program = unsafe { gl.create_program() }
        .map_err(|err| anyhow::anyhow!("create image program: {err}"))?;
    let vs = unsafe { gl.create_shader(glow::VERTEX_SHADER) }
        .map_err(|err| anyhow::anyhow!("create image vertex shader: {err}"))?;
    let fs = unsafe { gl.create_shader(glow::FRAGMENT_SHADER) }
        .map_err(|err| anyhow::anyhow!("create image fragment shader: {err}"))?;

    unsafe {
        gl.shader_source(vs, vs_src);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("image vertex shader compile error: {log}");
        }

        gl.shader_source(fs, fs_src);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.delete_program(program);
            anyhow::bail!("image fragment shader compile error: {log}");
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            anyhow::bail!("image program link error: {log}");
        }
    }

    Ok(program)
}
