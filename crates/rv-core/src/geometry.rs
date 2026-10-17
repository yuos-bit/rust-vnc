use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn aspect(self) -> f64 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f64 / self.height as f64
        }
    }
}

/// Largest rectangle with the source aspect ratio that fits inside the
/// destination box. Integer rounding never exceeds the destination.
pub fn aspect_fit(source: Size, destination: Size) -> Size {
    if source.width == 0 || source.height == 0 {
        return Size::new(0, 0);
    }
    if destination.width == 0 || destination.height == 0 {
        return Size::new(0, 0);
    }

    let scale_w = destination.width as f64 / source.width as f64;
    let scale_h = destination.height as f64 / source.height as f64;
    let scale = scale_w.min(scale_h);

    let width = ((source.width as f64 * scale).floor() as u32).clamp(1, destination.width);
    let height = ((source.height as f64 * scale).floor() as u32).clamp(1, destination.height);
    Size::new(width, height)
}

/// Converts a point expressed in source pixels into normalized 0..=1
/// coordinates, clamped to the source rectangle.
pub fn normalize_point(x: f64, y: f64, source: Size) -> (f64, f64) {
    if source.width == 0 || source.height == 0 {
        return (0.0, 0.0);
    }
    (
        (x / source.width as f64).clamp(0.0, 1.0),
        (y / source.height as f64).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_16_9_into_square() {
        let fitted = aspect_fit(Size::new(1920, 1080), Size::new(800, 800));
        assert_eq!(fitted, Size::new(800, 450));
    }

    #[test]
    fn fits_4_3_into_wide_box() {
        let fitted = aspect_fit(Size::new(1024, 768), Size::new(1920, 800));
        assert_eq!(fitted, Size::new(1066, 800));
    }

    #[test]
    fn portrait_source_into_landscape_box() {
        let fitted = aspect_fit(Size::new(1080, 1920), Size::new(1600, 900));
        assert_eq!(fitted, Size::new(506, 900));
    }

    #[test]
    fn never_exceeds_destination() {
        for (sw, sh, dw, dh) in [
            (1920u32, 1080u32, 333u32, 777u32),
            (2560, 1440, 1000, 1000),
            (800, 600, 801, 601),
        ] {
            let fit = aspect_fit(Size::new(sw, sh), Size::new(dw, dh));
            assert!(fit.width <= dw && fit.height <= dh);
            assert!(fit.width > 0 && fit.height > 0);
        }
    }

    #[test]
    fn normalization_clamps() {
        let (x, y) = normalize_point(960.0, 540.0, Size::new(1920, 1080));
        assert!((x - 0.5).abs() < f64::EPSILON);
        assert!((y - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            normalize_point(-10.0, 9999.0, Size::new(100, 100)),
            (0.0, 1.0)
        );
    }
}
