use std::{env, fs, path::PathBuf};

fn icon(size: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (
                (x as f64 + 0.5) / size as f64,
                (y as f64 + 0.5) / size as f64,
            );
            let inside = (0.04..0.96).contains(&u) && (0.04..0.96).contains(&v);
            let wall = ((0.18..0.25).contains(&u) && (0.19..0.82).contains(&v))
                || ((0.18..0.82).contains(&u) && (0.75..0.82).contains(&v))
                || ((0.75..0.82).contains(&u) && (0.19..0.82).contains(&v))
                || ((0.18..0.5).contains(&u) && (0.19..0.26).contains(&v));
            let furniture = (0.37..0.65).contains(&u) && (0.4..0.64).contains(&v);
            let rgba = if !inside {
                [0, 0, 0, 0]
            } else if wall {
                [237, 242, 229, 255]
            } else if furniture {
                [231, 178, 91, 255]
            } else {
                [28, 103, 94, 255]
            };
            bytes.extend(rgba);
        }
    }
    bytes
}
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("icon.rgba"), icon(64)).unwrap();
    let sizes = [16u32, 20, 24, 32, 48, 64, 128];
    let mut images = vec![];
    for size in sizes {
        let pixels = icon(size);
        let mask_stride = size.div_ceil(32) * 4;
        let mut dib = vec![];
        dib.extend(40u32.to_le_bytes());
        dib.extend((size as i32).to_le_bytes());
        dib.extend((size as i32 * 2).to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(32u16.to_le_bytes());
        dib.extend([0; 24]);
        for y in (0..size).rev() {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                dib.extend([pixels[i + 2], pixels[i + 1], pixels[i], pixels[i + 3]]);
            }
        }
        dib.extend(vec![0; (mask_stride * size) as usize]);
        images.push(dib);
    }
    let mut ico = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + sizes.len() * 16;
    for (i, size) in sizes.iter().enumerate() {
        ico.extend([*size as u8, *size as u8, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((images[i].len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += images[i].len();
    }
    for image in images {
        ico.extend(image);
    }
    let path = out.join("roomplanner.ico");
    fs::write(&path, ico).unwrap();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(path.to_str().unwrap())
            .set("ProductName", "RoomPlanner")
            .set("FileDescription", "RoomPlanner")
            .set(
                "LegalCopyright",
                "Copyright (c) 2025 siarheisinelnikau. GPLv3 License.",
            )
            .compile()
            .unwrap();
    }
}
