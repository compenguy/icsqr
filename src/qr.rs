//! QR code generation for display, clipboard copy, and PNG export.

use image::{ImageFormat, Luma, Rgba, RgbaImage};
use qrcode::QrCode;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::io::Cursor;

/// A rendered QR code ready for the Slint UI and for PNG serialization.
pub struct GeneratedQr {
    pub image: Image,
    pub png_bytes: Vec<u8>,
}

/// Encode `data` as a QR code.
///
/// The bitmap is generated at 280×280 pixels (a good balance of scan reliability
/// and file size). The Slint UI scales this image to fit its container.
pub fn generate_qr(data: &str) -> Result<GeneratedQr, String> {
    let code = QrCode::new(data.as_bytes()).map_err(|err| format!("QR generation failed: {err}"))?;
    let qr = code
        .render::<Luma<u8>>()
        .min_dimensions(280, 280)
        .max_dimensions(280, 280)
        .build();

    let (width, height) = qr.dimensions();
    let rgba = RgbaImage::from_fn(width, height, |x, y| {
        let value = qr.get_pixel(x, y).0[0];
        Rgba([value, value, value, 255])
    });

    let mut png_bytes = Vec::new();
    rgba.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|err| format!("Failed to encode PNG: {err}"))?;

    // Slint expects premultiplied RGBA for in-memory images.
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let pixels = buffer.make_mut_slice();
    for (target, source) in pixels.iter_mut().zip(rgba.pixels()) {
        *target = Rgba8Pixel {
            r: source[0],
            g: source[1],
            b: source[2],
            a: source[3],
        };
    }

    Ok(GeneratedQr {
        image: Image::from_rgba8(buffer),
        png_bytes,
    })
}
