//! On-disk cache of user avatars (`{cache dir}/avatars/`), so pictures show at once on startup
//! and ADO is asked again at most once every [`MAX_AGE`] per person. Files are named after the
//! subject descriptor; the media type is sniffed from the bytes when serving them.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::config::project_dirs;
use crate::error::CoreError;

pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The cached image, unless missing or older than [`MAX_AGE`].
pub fn load(descriptor: &str) -> Option<Vec<u8>> {
    let path = path(descriptor)?;
    let age = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())?;
    if age > MAX_AGE {
        return None;
    }
    std::fs::read(path).ok()
}

pub fn save(descriptor: &str, bytes: &[u8]) -> Result<(), CoreError> {
    let path = path(descriptor).ok_or_else(|| CoreError::Config("no cache directory".into()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Media type from the file signature (ADO serves PNG; JPEG/GIF/WebP for uploaded photos).
pub fn content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => "application/octet-stream",
    }
}

fn path(descriptor: &str) -> Option<PathBuf> {
    project_dirs().map(|d| d.cache_dir().join("avatars").join(file_name(descriptor)))
}

/// Descriptors are `aad.<base64>`, which may contain `/` and `+`.
fn file_name(descriptor: &str) -> String {
    descriptor
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_image_types() {
        assert_eq!(content_type(b"\x89PNG\r\n\x1a\n...."), "image/png");
        assert_eq!(content_type(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(content_type(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(content_type(b"<html>"), "application/octet-stream");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("aad.ab/c+d=="), "aad.ab_c_d__");
        assert!(!file_name("../../etc").contains('/'));
    }
}
