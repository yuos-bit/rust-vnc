//! Screen capture.
//!
//! The agent captures the *virtual desktop* (all monitors stitched by Windows)
//! at its native pixel size. Downscaling is opt-in and always preserves the
//! aspect ratio, so the controlled PC's display geometry is never altered.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ExtendedColorType, RgbImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureConfig {
    /// 0 keeps the native width. Any other value downscales proportionally.
    pub max_width: u32,
    /// JPEG quality, clamped to 20..=95.
    pub quality: u8,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            max_width: 0,
            quality: 70,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    /// Top-down BGRA, 4 bytes per pixel.
    pub bgra: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

/// Captures and JPEG-encodes one frame.
pub fn capture_jpeg(config: &CaptureConfig) -> Result<EncodedFrame, CaptureError> {
    let raw = capture_virtual_screen()?;
    encode_jpeg(&raw, config)
}

/// Converts BGRA -> RGB, optionally downscales (aspect preserved) and encodes.
pub fn encode_jpeg(raw: &RawFrame, config: &CaptureConfig) -> Result<EncodedFrame, CaptureError> {
    if raw.width == 0 || raw.height == 0 {
        return Err(CaptureError::EmptyFrame);
    }
    let expected = raw.width as usize * raw.height as usize * 4;
    if raw.bgra.len() < expected {
        return Err(CaptureError::BadBuffer {
            expected,
            actual: raw.bgra.len(),
        });
    }

    let mut rgb = Vec::with_capacity(raw.width as usize * raw.height as usize * 3);
    let (pixels, _) = raw.bgra.as_chunks::<4>();
    for px in pixels {
        rgb.push(px[2]);
        rgb.push(px[1]);
        rgb.push(px[0]);
    }
    let image = RgbImage::from_raw(raw.width, raw.height, rgb).ok_or(CaptureError::BadBuffer {
        expected,
        actual: raw.width as usize * raw.height as usize * 3,
    })?;

    let (target_w, target_h) = scaled_size(raw.width, raw.height, config.max_width);
    let image = if target_w != raw.width || target_h != raw.height {
        image::imageops::resize(&image, target_w, target_h, FilterType::Triangle)
    } else {
        image
    };

    let quality = config.quality.clamp(20, 95);
    let mut jpeg = Vec::with_capacity(64 * 1024);
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, quality);
    encoder
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| CaptureError::Encode(e.to_string()))?;

    Ok(EncodedFrame {
        width: image.width(),
        height: image.height(),
        jpeg,
    })
}

fn scaled_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if max_width == 0 || width <= max_width {
        return (width, height);
    }
    let new_height = ((height as u64 * max_width as u64) / width.max(1) as u64).max(1) as u32;
    (max_width, new_height)
}

#[cfg(windows)]
mod platform {
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HDC, HGDIOBJ,
        ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    use super::{CaptureError, RawFrame};

    /// 256 MP x 4 bytes guard: prevents a bogus virtual-screen metric from
    /// turning into a multi-gigabyte allocation.
    const MAX_PIXELS: u64 = 64 * 1024 * 1024;

    struct DcGuard(HDC);
    impl Drop for DcGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    ReleaseDC(null_mut(), self.0);
                }
            }
        }
    }

    struct MemDcGuard(HDC);
    impl Drop for MemDcGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DeleteDC(self.0);
                }
            }
        }
    }

    struct BitmapGuard(windows_sys::Win32::Graphics::Gdi::HBITMAP);
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DeleteObject(self.0 as HGDIOBJ);
                }
            }
        }
    }

    pub fn enable_dpi_awareness() {
        // Fails harmlessly if the manifest already set awareness.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    pub fn capture_virtual_screen() -> Result<RawFrame, CaptureError> {
        enable_dpi_awareness();

        let (mut x, mut y) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            )
        };
        let (mut width, mut height) = unsafe {
            (
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        if width <= 0 || height <= 0 {
            x = 0;
            y = 0;
            width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
            height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        }
        let width_u = u32::try_from(width).map_err(|_| CaptureError::Unsupported)?;
        let height_u = u32::try_from(height).map_err(|_| CaptureError::Unsupported)?;
        if width_u == 0 || height_u == 0 {
            return Err(CaptureError::NoScreen);
        }
        if u64::from(width_u) * u64::from(height_u) > MAX_PIXELS {
            return Err(CaptureError::TooLarge {
                width: width_u,
                height: height_u,
            });
        }

        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return Err(CaptureError::Platform(
                    "GetDC(NULL) returned null".to_string(),
                ));
            }
            let screen = DcGuard(screen_dc);

            let mem_dc = CreateCompatibleDC(screen.0);
            if mem_dc.is_null() {
                return Err(CaptureError::Platform(
                    "CreateCompatibleDC failed".to_string(),
                ));
            }
            let mem = MemDcGuard(mem_dc);

            let bitmap = CreateCompatibleBitmap(screen.0, width, height);
            if bitmap.is_null() {
                return Err(CaptureError::Platform(
                    "CreateCompatibleBitmap failed".to_string(),
                ));
            }
            let bitmap = BitmapGuard(bitmap);

            let old = SelectObject(mem.0, bitmap.0 as HGDIOBJ);
            if old.is_null() {
                return Err(CaptureError::Platform("SelectObject failed".to_string()));
            }

            let blt = BitBlt(
                mem.0,
                0,
                0,
                width,
                height,
                screen.0,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            );
            // Deselect before GetDIBits, as required by the GDI contract.
            SelectObject(mem.0, old);
            if blt == 0 {
                return Err(CaptureError::Platform("BitBlt failed".to_string()));
            }

            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height requests a top-down DIB.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            };

            let len = width_u as usize * height_u as usize * 4;
            let mut bgra = vec![0u8; len];
            let lines = GetDIBits(
                mem.0,
                bitmap.0,
                0,
                height_u,
                bgra.as_mut_ptr() as *mut core::ffi::c_void,
                &mut info,
                DIB_RGB_COLORS,
            );
            if lines == 0 {
                return Err(CaptureError::Platform("GetDIBits failed".to_string()));
            }

            Ok(RawFrame {
                width: width_u,
                height: height_u,
                origin_x: x,
                origin_y: y,
                bgra,
            })
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{CaptureError, RawFrame};

    pub fn enable_dpi_awareness() {}

    pub fn capture_virtual_screen() -> Result<RawFrame, CaptureError> {
        Err(CaptureError::Unsupported)
    }
}

pub use platform::{capture_virtual_screen, enable_dpi_awareness};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("screen capture is only implemented on Windows in this build")]
    Unsupported,
    #[error("no screen was reported by the operating system")]
    NoScreen,
    #[error("screen is too large to capture: {width}x{height}")]
    TooLarge { width: u32, height: u32 },
    #[error("empty frame")]
    EmptyFrame,
    #[error("buffer size mismatch: expected at least {expected}, got {actual}")]
    BadBuffer { expected: usize, actual: usize },
    #[error("capture backend error: {0}")]
    Platform(String),
    #[error("JPEG encoding failed: {0}")]
    Encode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_preserves_aspect() {
        assert_eq!(scaled_size(1920, 1080, 0), (1920, 1080));
        assert_eq!(scaled_size(1920, 1080, 1280), (1280, 720));
        assert_eq!(scaled_size(1920, 1080, 2000), (1920, 1080));
        assert_eq!(scaled_size(1000, 333, 100), (100, 33));
        assert_eq!(scaled_size(1, 1, 0), (1, 1));
    }

    #[test]
    fn encodes_a_tiny_bgra_frame() {
        let raw = RawFrame {
            width: 2,
            height: 2,
            origin_x: 0,
            origin_y: 0,
            bgra: vec![
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
            ],
        };
        let encoded = encode_jpeg(
            &raw,
            &CaptureConfig {
                max_width: 0,
                quality: 80,
            },
        )
        .unwrap();
        assert_eq!((encoded.width, encoded.height), (2, 2));
        assert!(encoded.jpeg.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn rejects_short_buffers() {
        let raw = RawFrame {
            width: 4,
            height: 4,
            origin_x: 0,
            origin_y: 0,
            bgra: vec![0; 8],
        };
        assert!(matches!(
            encode_jpeg(&raw, &CaptureConfig::default()),
            Err(CaptureError::BadBuffer { .. })
        ));
    }
}
