use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=docs/teletipo128x128.png");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    if let Err(err) = embed_windows_icon() {
        panic!("failed to embed Windows icon resource: {err}");
    }
}

fn embed_windows_icon() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let png_path = manifest_dir.join("docs/teletipo128x128.png");
    let ico_path = out_dir.join("teletipo.ico");

    let image = image::open(&png_path)?;
    image.save_with_format(&ico_path, image::ImageFormat::Ico)?;

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_string_lossy().as_ref());
    res.compile()?;

    Ok(())
}
