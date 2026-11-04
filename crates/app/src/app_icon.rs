//! The app icon, drawn in code so no image decoder is needed: a rounded board whose eyes are two
//! kanban cards (one column fuller than the other) above a smile, on a 28-unit grid.
//!
//! [`icon`] is the window / taskbar icon at runtime. The files in `crates/app/icons/` (bundle
//! icons for `dx bundle`, and `icon.ico` that `build.rs` embeds in the Windows `.exe`) are rendered
//! from the same code; regenerate them after changing the drawing with
//! `cargo test -p app write_icon_files -- --ignored`.

use dioxus::desktop::tao::window::Icon;

/// Runtime window icon size.
const SIZE: u32 = 64;
/// Samples per axis per pixel, for anti-aliasing.
const SS: u32 = 4;
const ACCENT: [u8; 3] = [0x7C, 0x9B, 0xFF];
const INK: [u8; 3] = [0x0B, 0x10, 0x20];

pub fn icon() -> Option<Icon> {
    Icon::from_rgba(rgba(SIZE), SIZE, SIZE)
        .inspect_err(|e| tracing::warn!(error = %e, "no window icon"))
        .ok()
}

/// The icon as `size`×`size` RGBA pixels (transparent outside the board).
pub fn rgba(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let scale = 28.0 / f64::from(size);
    for py in 0..size {
        for px in 0..size {
            let (mut board, mut ink) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    // Sample position in the 28-unit design grid.
                    let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(SS)) * scale;
                    let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(SS)) * scale;
                    if rounded_rect(x, y, 1.0, 1.0, 26.0, 26.0, 7.0) {
                        board += 1;
                        if is_face(x, y) {
                            ink += 1;
                        }
                    }
                }
            }
            let t = f64::from(ink) / f64::from(board.max(1));
            for c in 0..3 {
                let v = f64::from(ACCENT[c]) * (1.0 - t) + f64::from(INK[c]) * t;
                out.push(v.round() as u8);
            }
            out.push((f64::from(board) / f64::from(SS * SS) * 255.0).round() as u8);
        }
    }
    out
}

/// Eyes (two cards of different heights) and the smile.
fn is_face(x: f64, y: f64) -> bool {
    let left_eye = rounded_rect(x, y, 8.0, 6.5, 4.0, 8.5, 1.5);
    let right_eye = rounded_rect(x, y, 16.0, 6.5, 4.0, 5.5, 1.5);
    // Lower arc of a circle centred above the mouth.
    let (cx, cy, r) = (14.0, 13.5, 7.5);
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let smile = (d - r).abs() <= 1.1 && y >= cy + 4.6;
    left_eye || right_eye || smile
}

fn rounded_rect(x: f64, y: f64, left: f64, top: f64, w: f64, h: f64, r: f64) -> bool {
    let qx = (x - (left + w / 2.0)).abs() - (w / 2.0 - r);
    let qy = (y - (top + h / 2.0)).abs() - (h / 2.0 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) <= r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_corners_opaque_board() {
        let px = rgba(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert_eq!(px[3], 0, "top-left corner is outside the rounded board");
        // A pixel on the cheek: accent colour, fully opaque.
        let i = ((24 * 32 + 6) * 4) as usize;
        assert_eq!(&px[i..i + 4], &[ACCENT[0], ACCENT[1], ACCENT[2], 255]);
    }

    /// Writes `crates/app/icons/` (bundle PNGs and the Windows `.ico`). Run on demand.
    #[test]
    #[ignore = "writes files; run after changing the icon"]
    fn write_icon_files() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
        std::fs::create_dir_all(&dir).unwrap();
        for size in [32, 128, 256] {
            std::fs::write(dir.join(format!("{size}x{size}.png")), png(size)).unwrap();
        }
        std::fs::write(dir.join("icon.ico"), ico(&[16, 24, 32, 48, 64, 128, 256])).unwrap();
    }

    /// An ICO whose images are PNGs (supported since Windows Vista).
    fn ico(sizes: &[u32]) -> Vec<u8> {
        let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect();
        let mut out = Vec::new();
        out.extend([0, 0, 1, 0]);
        out.extend((sizes.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * sizes.len() as u32;
        for (&size, image) in sizes.iter().zip(&images) {
            let dim = if size >= 256 { 0 } else { size as u8 };
            out.extend([dim, dim, 0, 0]);
            out.extend(1u16.to_le_bytes()); // colour planes
            out.extend(32u16.to_le_bytes()); // bits per pixel
            out.extend((image.len() as u32).to_le_bytes());
            out.extend(offset.to_le_bytes());
            offset += image.len() as u32;
        }
        for image in images {
            out.extend(image);
        }
        out
    }

    /// A minimal RGBA PNG with an uncompressed (stored) deflate stream: larger than it needs to
    /// be, but it avoids an encoder dependency for files that are rarely regenerated.
    fn png(size: u32) -> Vec<u8> {
        let px = rgba(size);
        let stride = (size * 4) as usize;
        let mut raw = Vec::with_capacity((stride + 1) * size as usize);
        for row in px.chunks(stride) {
            raw.push(0); // filter: none
            raw.extend(row);
        }
        let mut ihdr = Vec::new();
        ihdr.extend(size.to_be_bytes());
        ihdr.extend(size.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]); // 8-bit RGBA
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &zlib_stored(&raw));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let crc = crc32(&out[start..]);
        out.extend(crc.to_be_bytes());
    }

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = data.chunks(65_535).collect();
        for (i, block) in blocks.iter().enumerate() {
            out.push(u8::from(i + 1 == blocks.len()));
            let len = block.len() as u16;
            out.extend(len.to_le_bytes());
            out.extend((!len).to_le_bytes());
            out.extend(*block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in data {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        out.extend(((b << 16) | a).to_be_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
}
