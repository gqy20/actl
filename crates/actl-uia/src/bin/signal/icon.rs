//! Pixel-sized tray glyph: a rounded frame and a status dot, supersampled per size.
use windows::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};
use windows::core::Result;

pub fn pixels(size: usize, rgb: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut sums = [0u32; 4];
            for sy in 0..8 {
                for sx in 0..8 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 8.) * 16. / size as f32;
                    let py = (y as f32 + (sy as f32 + 0.5) / 8.) * 16. / size as f32;
                    let qx = (px - 7.5).abs() - 3.5;
                    let qy = (py - 7.5).abs() - 3.5;
                    let distance = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - 2.;
                    let dot = (px - 11.5).hypot(py - 11.5);
                    let color = if dot < 2.5 {
                        Some(rgb)
                    } else if dot < 3.5 {
                        None
                    } else if distance.abs() < 0.8 {
                        Some(0xDCE1E5)
                    } else {
                        None
                    };
                    if let Some(c) = color {
                        sums[0] += c & 255;
                        sums[1] += (c >> 8) & 255;
                        sums[2] += (c >> 16) & 255;
                        sums[3] += 255;
                    }
                }
            }
            for i in 0..4 {
                pixels[(y * size + x) * 4 + i] = (sums[i] / 64) as u8;
            }
        }
    }
    pixels
}
pub unsafe fn create(size: i32, rgb: u32) -> Result<HICON> {
    let data = pixels(size as usize, rgb);
    let bitmap = CreateBitmap(size, size, 1, 32, Some(data.as_ptr().cast()));
    if bitmap.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let mask_bits = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
    if mask.is_invalid() {
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        return Err(windows::core::Error::from_thread());
    }
    let result = CreateIconIndirect(&ICONINFO {
        fIcon: true.into(),
        hbmColor: bitmap,
        hbmMask: mask,
        ..Default::default()
    });
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteObject(HGDIOBJ(mask.0));
    result
}
#[cfg(test)]
mod tests {
    #[test]
    fn native_sizes_have_transparency_and_smooth_edges() {
        for size in [16, 20, 24, 32] {
            let pixels = super::pixels(size, 0xD4B879);
            assert_eq!(pixels.len(), size * size * 4);
            assert_eq!(pixels[3], 0);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        }
    }
}
