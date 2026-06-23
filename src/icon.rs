// icon.rs — monochrome template status icon (22×22 PNG).
//
// Faithful port of icon.go: a ring outlines a gauge; a solid sector inside
// fills clockwise to the usage percentage. All pixels are black with varying
// alpha — AppKit tints template images to match the menu bar.

use image::{ImageBuffer, ImageEncoder, Rgba, RgbaImage};
use std::f64::consts::PI;
use std::io::Cursor;

const SIZE: u32 = 22;

/// Generate the gauge icon for `fill_pct` (clamped to [0,100]); -1 => empty ring.
fn gauge_icon_bytes(fill_pct: i32) -> Vec<u8> {
    let center = SIZE as f64 / 2.0 - 0.5; // = 10.5
    let outer_r = SIZE as f64 / 2.0 - 1.0; // = 10
    let inner_r = outer_r - 2.5; // = 7.5

    let mut img: RgbaImage = ImageBuffer::new(SIZE, SIZE);
    let black = Rgba([0u8, 0, 0, 255]);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            // Ring band [innerR, outerR].
            if dist >= inner_r && dist <= outer_r {
                let mut alpha = 255.0;
                if dist > outer_r - 1.0 {
                    alpha *= outer_r - dist; // fade out at outer edge
                } else if dist < inner_r + 1.0 {
                    alpha *= dist - inner_r; // fade in at inner edge
                }
                let alpha = alpha.clamp(0.0, 255.0) as u8;
                img.put_pixel(x, y, Rgba([0, 0, 0, alpha]));
                continue;
            }

            // Interior fill: solid sector clockwise from top.
            if dist < inner_r && fill_pct > 0 {
                let angle = dy.atan2(dx) + PI / 2.0; // 0 at top
                let angle = if angle < 0.0 { angle + 2.0 * PI } else { angle };
                let threshold = 2.0 * PI * fill_pct as f64 / 100.0;
                if angle <= threshold {
                    img.put_pixel(x, y, black);
                }
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new_with_quality(
        &mut buf,
        image::codecs::png::CompressionType::Default,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(img.as_raw(), SIZE, SIZE, image::ExtendedColorType::Rgba8)
    .ok();
    buf.into_inner()
}

/// Icon for a given worst-usage percentage (clamped).
pub fn usage_icon_bytes(monthly_pct: i32) -> Vec<u8> {
    gauge_icon_bytes(clamp_percent(monthly_pct))
}

/// Empty ring for unconfigured / loading states.
pub fn neutral_icon_bytes() -> Vec<u8> {
    gauge_icon_bytes(-1)
}

fn clamp_percent(pct: i32) -> i32 {
    match pct {
        p if p < 0 => 0,
        p if p > 100 => 100,
        p => p,
    }
}
