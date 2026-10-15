//! Sender-supplied image bytes become a bounded PNG decoded by memory-safe code.
//! The GUI only ever decodes the PNG produced here. Pure transform: no I/O.
use image::{
    codecs::png, DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits,
};
use std::io::Cursor;

/// Decoded pixel budget: 16 megapixels, about 64 MiB as RGBA.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// Widest image kept; a reader column at 2x device pixels fits within it.
pub const MAX_WIDTH: u32 = 1600;

/// Why image bytes were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    /// Not PNG, JPEG, GIF or WebP by its magic bytes (SVG is never accepted).
    Unsupported,
    /// Over [`MAX_PIXELS`], or over a decoder size or allocation limit while decoding.
    TooLarge,
    /// Corrupt, truncated or zero-sized, or the PNG could not be written.
    Invalid,
}

impl ImageError {
    /// Stable snake_case code for error reporting: `unsupported`, `too_large`
    /// or `invalid`.
    pub fn code(self) -> &'static str {
        match self {
            ImageError::Unsupported => "unsupported",
            ImageError::TooLarge => "too_large",
            ImageError::Invalid => "invalid",
        }
    }
}

/// An RGBA PNG re-encoded from sender bytes, with its size in pixels.
pub struct NormalizedImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decodes PNG, JPEG, GIF or WebP bytes and re-encodes them as an RGBA PNG no
/// wider than [`MAX_WIDTH`].
///
/// The format comes from the magic bytes, not a declared content type. A
/// camera's EXIF orientation is applied, then wider images are scaled down
/// keeping their aspect ratio; animations keep their first frame.
pub fn normalize_image(bytes: &[u8]) -> Result<NormalizedImage, ImageError> {
    let format = image::guess_format(bytes).map_err(|_| ImageError::Unsupported)?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
    ) {
        return Err(ImageError::Unsupported);
    }
    let reader = |bytes| {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader
    };
    // Read the header first, so an oversized image is refused before any
    // pixel buffer is allocated.
    let (width, height) = reader(bytes)
        .into_dimensions()
        .map_err(|_| ImageError::Invalid)?;
    if width == 0 || height == 0 {
        return Err(ImageError::Invalid);
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ImageError::TooLarge);
    }
    let refused = |error| match error {
        image::ImageError::Limits(_) => ImageError::TooLarge,
        _ => ImageError::Invalid,
    };
    // Animated GIF and WebP decode to their first frame.
    let mut decoder = reader(bytes).into_decoder().map_err(refused)?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(refused)?;
    decoded.apply_orientation(orientation);
    if decoded.width() > MAX_WIDTH {
        let height = ((u64::from(decoded.height()) * u64::from(MAX_WIDTH))
            / u64::from(decoded.width()))
        .max(1) as u32;
        decoded = decoded.resize_exact(MAX_WIDTH, height, image::imageops::FilterType::Triangle);
    }
    let (width, height) = decoded.dimensions();
    let rgba = decoded.into_rgba8();
    let mut png = Vec::new();
    let encoder = png::PngEncoder::new_with_quality(
        &mut png,
        png::CompressionType::Fast,
        png::FilterType::Adaptive,
    );
    image::ImageEncoder::write_image(
        encoder,
        rgba.as_raw(),
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|_| ImageError::Invalid)?;
    Ok(NormalizedImage { png, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn accepted_formats_become_bounded_png() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif] {
            let result = normalize_image(&encoded(40, 20, format)).unwrap();
            assert_eq!((result.width, result.height), (40, 20));
            assert_eq!(image::guess_format(&result.png).unwrap(), ImageFormat::Png);
        }
        // EXIF orientation 6 (rotate 90 degrees clockwise) turns 40×20 into 20×40.
        let mut camera = encoded(40, 20, ImageFormat::Jpeg);
        let exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut segment = vec![0xff, 0xe1, 0, (exif.len() + 2) as u8];
        segment.extend_from_slice(exif);
        camera.splice(2..2, segment);
        let turned = normalize_image(&camera).unwrap();
        assert_eq!((turned.width, turned.height), (20, 40));
        let wide = normalize_image(&encoded(3200, 100, ImageFormat::Png)).unwrap();
        assert_eq!((wide.width, wide.height), (MAX_WIDTH, 50));
    }

    #[test]
    fn other_bytes_are_refused() {
        assert_eq!(
            normalize_image(b"<svg xmlns='http://www.w3.org/2000/svg'/>").err(),
            Some(ImageError::Unsupported)
        );
        assert_eq!(
            normalize_image(b"not an image").err(),
            Some(ImageError::Unsupported)
        );
        let mut truncated = encoded(64, 64, ImageFormat::Png);
        truncated.truncate(60);
        assert!(normalize_image(&truncated).is_err());
        let mut huge = encoded(1, 1, ImageFormat::Png);
        // IHDR width/height: claim 20000x20000 pixels.
        huge[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        huge[20..24].copy_from_slice(&20_000u32.to_be_bytes());
        assert!(normalize_image(&huge).is_err());
        let bmp = b"\x42\x4d\x3a\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00";
        assert_eq!(normalize_image(bmp).err(), Some(ImageError::Unsupported));
    }
}
