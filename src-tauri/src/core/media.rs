use crate::core::platform::MediaAttachment;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use std::fs;
use std::path::Path;

const MAX_INITIAL_EDGE: u32 = 2560;
const MIN_DOWNSCALE_EDGE: u32 = 640;
const QUALITY_STEPS: [u8; 5] = [88, 80, 72, 64, 55];

fn is_compressible_image(mime_type: &str) -> bool {
    matches!(mime_type, "image/jpeg" | "image/png" | "image/webp")
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{} KB", (bytes as f64 / 1_000.0).round() as usize)
    } else {
        format!("{bytes} B")
    }
}

fn file_name_for_note(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string()
}

fn composite_over_white(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let alpha = px[3] as f32 / 255.0;
        let inv = 1.0 - alpha;
        let r = (px[0] as f32 * alpha + 255.0 * inv).round() as u8;
        let g = (px[1] as f32 * alpha + 255.0 * inv).round() as u8;
        let b = (px[2] as f32 * alpha + 255.0 * inv).round() as u8;
        rgb.put_pixel(x, y, Rgb([r, g, b]));
    }
    rgb
}

fn encode_jpeg(rgb: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut out, quality);
    rgb.write_with_encoder(encoder)
        .map_err(|err| format!("Failed to encode JPEG: {err}"))?;
    Ok(out)
}

fn compress_image_to_limit(raw_bytes: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut img = image::load_from_memory(raw_bytes)
        .map_err(|err| format!("Failed to decode image for compression: {err}"))?;

    if img.width() > MAX_INITIAL_EDGE || img.height() > MAX_INITIAL_EDGE {
        img = img.resize(MAX_INITIAL_EDGE, MAX_INITIAL_EDGE, FilterType::Lanczos3);
    }

    let rgb = composite_over_white(&img);
    let mut best_attempt: Vec<u8> = Vec::new();

    for quality in QUALITY_STEPS {
        let encoded = encode_jpeg(&rgb, quality)?;
        if encoded.len() <= limit {
            return Ok(encoded);
        }
        best_attempt = encoded;
    }

    // Still above the ceiling at quality 55 — iteratively downscale by 0.85x down to 640px.
    let mut current_img = img;
    loop {
        let w = current_img.width();
        let h = current_img.height();
        let longest = w.max(h);
        if longest <= MIN_DOWNSCALE_EDGE {
            break;
        }

        let target_longest = ((longest as f32) * 0.85)
            .round()
            .max(MIN_DOWNSCALE_EDGE as f32) as u32;
        if target_longest >= longest {
            break;
        }

        current_img = current_img.resize(target_longest, target_longest, FilterType::Lanczos3);
        let scaled_rgb = composite_over_white(&current_img);

        for quality in [72u8, 60u8] {
            let encoded = encode_jpeg(&scaled_rgb, quality)?;
            if encoded.len() <= limit {
                return Ok(encoded);
            }
            best_attempt = encoded;
        }
    }

    Ok(best_attempt)
}

/// Prepares media attachments for a specific platform's `max_image_bytes` ceiling.
/// Static images exceeding `max_image_bytes` are re-encoded as JPEG in memory without
/// mutating the original files on disk; GIFs, videos, and under-limit images are untouched.
pub fn prepare_media_for_platform(
    media: &[MediaAttachment],
    max_image_bytes: Option<usize>,
) -> Result<(Vec<MediaAttachment>, Vec<String>), String> {
    prepare_media_for_platform_with_options(media, max_image_bytes, true)
}

pub fn prepare_media_for_platform_with_options(
    media: &[MediaAttachment],
    max_image_bytes: Option<usize>,
    auto_compress: bool,
) -> Result<(Vec<MediaAttachment>, Vec<String>), String> {
    let Some(limit) = max_image_bytes else {
        return Ok((media.to_vec(), Vec::new()));
    };

    let mut prepared = Vec::with_capacity(media.len());
    let mut notes = Vec::new();

    for attachment in media {
        if !is_compressible_image(&attachment.mime_type) {
            prepared.push(attachment.clone());
            continue;
        }

        let raw_bytes = match attachment.bytes {
            Some(ref data) => data.clone(),
            None => fs::read(&attachment.path).map_err(|err| {
                format!(
                    "Failed to read attachment '{}': {err}",
                    attachment.path.display()
                )
            })?,
        };

        let original_len = raw_bytes.len();
        if original_len <= limit {
            prepared.push(attachment.clone());
            continue;
        }

        let original_name = file_name_for_note(&attachment.path);
        if !auto_compress {
            return Err(format!(
                "Image '{original_name}' ({}) exceeds the {} limit (auto-compression is disabled in Settings).",
                format_bytes(original_len),
                format_bytes(limit)
            ));
        }

        let compressed = compress_image_to_limit(&raw_bytes, limit)?;
        let compressed_len = compressed.len();

        let mut updated = attachment.clone();
        updated.bytes = Some(compressed);
        updated.mime_type = "image/jpeg".to_string();
        updated.path = attachment.path.with_extension("jpg");
        prepared.push(updated);

        notes.push(format!(
            "Compressed {original_name} ({} → {})",
            format_bytes(original_len),
            format_bytes(compressed_len)
        ));
    }

    Ok((prepared, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use std::path::PathBuf;

    fn synthetic_noisy_png(width: u32, height: u32) -> Vec<u8> {
        let mut img = RgbImage::new(width, height);
        let mut state: u32 = 0x1234_5678;
        for (_, _, px) in img.enumerate_pixels_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = (state >> 24) as u8;
            let g = (state >> 16) as u8;
            let b = (state >> 8) as u8;
            *px = Rgb([r, g, b]);
        }
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgb8)
            .expect("encodes synthetic png");
        out
    }

    #[test]
    fn compresses_oversized_png_under_bluesky_limit() {
        let raw_png = synthetic_noisy_png(1400, 1400);
        assert!(
            raw_png.len() > 976_562,
            "expected synthetic PNG ({} bytes) to exceed 976,562 bytes",
            raw_png.len()
        );

        let attachment = MediaAttachment {
            path: PathBuf::from("photo.png"),
            mime_type: "image/png".to_string(),
            alt_text: Some("Test photo".to_string()),
            bytes: Some(raw_png),
        };

        let (result, notes) =
            prepare_media_for_platform(&[attachment], Some(976_562)).expect("compression succeeds");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mime_type, "image/jpeg");
        assert_eq!(result[0].path, PathBuf::from("photo.jpg"));
        assert_eq!(result[0].alt_text.as_deref(), Some("Test photo"));
        let compressed_bytes = result[0].bytes.as_ref().expect("compressed bytes present");
        assert!(
            compressed_bytes.len() <= 976_562,
            "expected compressed size ({}) <= 976,562",
            compressed_bytes.len()
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("Compressed photo.png"));
    }

    #[test]
    fn passes_through_small_image_unchanged() {
        let small_png = synthetic_noisy_png(64, 64);
        let original_len = small_png.len();
        let attachment = MediaAttachment {
            path: PathBuf::from("small.png"),
            mime_type: "image/png".to_string(),
            alt_text: None,
            bytes: Some(small_png.clone()),
        };

        let (result, notes) =
            prepare_media_for_platform(&[attachment], Some(976_562)).expect("succeeds");
        assert_eq!(result[0].mime_type, "image/png");
        assert_eq!(result[0].path, PathBuf::from("small.png"));
        assert_eq!(result[0].bytes.as_deref(), Some(small_png.as_slice()));
        assert_eq!(result[0].bytes.as_ref().map(Vec::len), Some(original_len));
        assert!(notes.is_empty());
    }

    #[test]
    fn rejects_oversized_image_when_auto_compress_disabled() {
        let raw_png = synthetic_noisy_png(1400, 1400);
        let attachment = MediaAttachment {
            path: PathBuf::from("large.png"),
            mime_type: "image/png".to_string(),
            alt_text: None,
            bytes: Some(raw_png),
        };

        let err = prepare_media_for_platform_with_options(&[attachment], Some(976_562), false)
            .expect_err("rejects oversized image when compression is disabled");
        assert!(err.contains("auto-compression is disabled"));
    }
}
