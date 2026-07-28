fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        embed_resource::compile("icon.rc", embed_resource::NONE);
    }

    // Convert the .ico to ARGB32 raw bytes for the ksni tray on Linux.
    ico_to_argb();

    println!("cargo:rerun-if-changed=icon.rc");
    println!("cargo:rerun-if-changed=assets/fxembed.ico");
}

fn ico_to_argb() {
    let (w, h, argb) = match image::open("assets/fxembed.ico") {
        Ok(img) => {
            let img = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();

            // Convert RGBA → ARGB (ksni::Icon uses ARGB32).
            let mut argb = Vec::with_capacity(rgba.len());
            for px in rgba.chunks(4) {
                argb.push(px[3]); // A
                argb.push(px[0]); // R
                argb.push(px[1]); // G
                argb.push(px[2]); // B
            }
            (w, h, argb)
        }
        Err(_) => {
            // Icon missing — write a 1×1 transparent pixel so include_bytes!
            // in tray.rs doesn't fail at compile time.
            (1u32, 1u32, vec![0, 0, 0, 0])
        }
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    // Simple binary layout: u32-LE width, u32-LE height, then ARGB bytes.
    let mut buf = Vec::with_capacity(8 + argb.len());
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(&argb);
    std::fs::write(format!("{out_dir}/icon.argb"), &buf).unwrap();
}
