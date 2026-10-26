//! Screenshots of the game window, so an assistant can see which panel, tab
//! or row is highlighted when the journal can't say.
//!
//! Only the game window is ever captured, never the desktop. On Windows the
//! window renders itself into an off-screen bitmap (`PrintWindow`), so
//! whatever covers it stays out of the picture. Everywhere else a stand-in
//! reports that capture is unavailable.

use std::io::Cursor;

use anyhow::{Result, bail};
use image::{
    DynamicImage, ImageEncoder, RgbaImage,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Something that can grab the game window's client area as pixels.
pub trait Screen: Send + Sync {
    fn capture(&self) -> Result<RgbaImage>;
}

/// A rectangle as fractions (0..1) of the game window, so it survives
/// resolution changes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    #[schemars(description = "Left edge, 0..1 of window width")]
    pub x: f64,
    #[schemars(description = "Top edge, 0..1 of window height")]
    pub y: f64,
    #[schemars(description = "Width, 0..1 of window width")]
    pub w: f64,
    #[schemars(description = "Height, 0..1 of window height")]
    pub h: f64,
}

impl Region {
    pub const FULL: Region = Region {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// Regions every config has. `[screen_regions]` in holtzman.toml can add
    /// more (or override these) for panels on a particular screen layout.
    pub fn builtin(name: &str) -> Option<Region> {
        let r = |x, y, w, h| Some(Region { x, y, w, h });
        match name {
            "full" => Some(Self::FULL),
            "center" => r(0.25, 0.15, 0.5, 0.7),
            "left" => r(0.0, 0.0, 0.5, 1.0),
            "right" => r(0.5, 0.0, 0.5, 1.0),
            "top" => r(0.0, 0.0, 1.0, 0.5),
            "bottom" => r(0.0, 0.5, 1.0, 0.5),
            _ => None,
        }
    }

    pub const BUILTIN_NAMES: &[&str] = &["full", "center", "left", "right", "top", "bottom"];

    pub fn validate(&self) -> Result<()> {
        let in_unit = |v: f64| (0.0..=1.0).contains(&v);
        if !(in_unit(self.x) && in_unit(self.y) && self.w > 0.0 && self.h > 0.0)
            || self.x + self.w > 1.0 + 1e-9
            || self.y + self.h > 1.0 + 1e-9
        {
            bail!("region {self:?} must lie within the window (fractions 0..1, w/h > 0)");
        }
        Ok(())
    }

    /// Pixel rectangle (x, y, w, h) inside a `width` x `height` image.
    fn pixels(&self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let px = |f: f64, of: u32| ((f * f64::from(of)).round() as u32).min(of);
        let (x, y) = (
            px(self.x, width).min(width - 1),
            px(self.y, height).min(height - 1),
        );
        let w = px(self.w, width).clamp(1, width - x);
        let h = px(self.h, height).clamp(1, height - y);
        (x, y, w, h)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Smaller; fine for orientation. The default.
    #[default]
    Jpeg,
    /// Lossless; sharper text, several times larger.
    Png,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
        }
    }
}

/// An encoded screenshot plus what it shows.
pub struct Shot {
    pub bytes: Vec<u8>,
    pub format: Format,
    /// Encoded image size.
    pub width: u32,
    pub height: u32,
    /// Game window client size the crop was taken from.
    pub window_width: u32,
    pub window_height: u32,
}

/// Crop `frame` to `region`, shrink it so neither side exceeds `max_edge`,
/// and encode it.
pub fn render(
    frame: &RgbaImage,
    region: Region,
    max_edge: u32,
    format: Format,
    jpeg_quality: u8,
) -> Result<Shot> {
    region.validate()?;
    let (fw, fh) = frame.dimensions();
    if fw == 0 || fh == 0 {
        bail!("captured an empty frame");
    }
    let (x, y, w, h) = region.pixels(fw, fh);
    let mut img = image::imageops::crop_imm(frame, x, y, w, h).to_image();
    let longest = w.max(h);
    if longest > max_edge {
        let scale = f64::from(max_edge) / f64::from(longest);
        let nw = ((f64::from(w) * scale).round() as u32).max(1);
        let nh = ((f64::from(h) * scale).round() as u32).max(1);
        // Catmull-Rom keeps HUD text edges crisper than bilinear.
        img = image::imageops::resize(&img, nw, nh, FilterType::CatmullRom);
    }
    let (width, height) = img.dimensions();
    let mut bytes = Vec::new();
    match format {
        Format::Png => PngEncoder::new(Cursor::new(&mut bytes)).write_image(
            img.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?,
        Format::Jpeg => {
            // JPEG has no alpha channel.
            let rgb = DynamicImage::ImageRgba8(img).into_rgb8();
            JpegEncoder::new_with_quality(Cursor::new(&mut bytes), jpeg_quality.clamp(1, 100))
                .write_image(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)?
        }
    }
    Ok(Shot {
        bytes,
        format,
        width,
        height,
        window_width: fw,
        window_height: fh,
    })
}

/// True when (nearly) every pixel is black: what capture returns for a game
/// that renders outside the reach of `PrintWindow`.
pub fn looks_black(frame: &RgbaImage) -> bool {
    let step = (frame.as_raw().len() / 4 / 4096).max(1);
    frame
        .pixels()
        .step_by(step)
        .all(|p| p.0[0] < 8 && p.0[1] < 8 && p.0[2] < 8)
}

/// Fraction (0..1) of sampled pixels that visibly differ between two frames.
/// Lets an agent tell "the key did nothing" from "the UI moved" without
/// looking at the picture. Frames of different sizes count as fully changed.
pub fn changed_fraction(a: &RgbaImage, b: &RgbaImage) -> f64 {
    if a.dimensions() != b.dimensions() || a.as_raw().is_empty() {
        return 1.0;
    }
    // ~16k samples is plenty and keeps this cheap on a 5-megapixel window.
    let step = (a.as_raw().len() / 4 / 16_384).max(1);
    let (mut seen, mut changed) = (0u32, 0u32);
    for (p, q) in a.pixels().zip(b.pixels()).step_by(step) {
        seen += 1;
        // Ignore small shimmer: compression-free, but the HUD glows and pulses.
        if (0..3).any(|i| p.0[i].abs_diff(q.0[i]) > 24) {
            changed += 1;
        }
    }
    f64::from(changed) / f64::from(seen)
}

/// Stand-in for platforms without capture.
pub struct Unsupported;

impl Screen for Unsupported {
    fn capture(&self) -> Result<RgbaImage> {
        bail!("screenshots are only supported on Windows")
    }
}

/// The real screen grabber for this platform.
pub fn platform_screen() -> std::sync::Arc<dyn Screen> {
    #[cfg(windows)]
    {
        std::sync::Arc::new(windows::WindowCapture::new())
    }
    #[cfg(not(windows))]
    {
        std::sync::Arc::new(Unsupported)
    }
}

#[cfg(windows)]
mod windows {
    use anyhow::{Result, bail};
    use image::RgbaImage;
    use windows_sys::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap,
            CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HDC,
            ReleaseDC, SRCCOPY, SelectObject,
        },
        Storage::Xps::{PW_CLIENTONLY, PrintWindow},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{GetForegroundWindow, PW_RENDERFULLCONTENT},
        },
    };

    use crate::input::windows::{client_area, game_window};

    pub struct WindowCapture;

    impl WindowCapture {
        pub fn new() -> Self {
            // Physical pixels, matching the input side.
            // SAFETY: process-wide setting; fails harmlessly if already set.
            unsafe {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            }
            Self
        }
    }

    impl super::Screen for WindowCapture {
        fn capture(&self) -> Result<RgbaImage> {
            let hwnd = game_window()?;
            let (ox, oy, w, h) = client_area(hwnd)?;
            // SAFETY: plain GDI calls on handles created and released here.
            unsafe {
                let screen = GetDC(std::ptr::null_mut());
                if screen.is_null() {
                    bail!("GetDC failed");
                }
                let mem = CreateCompatibleDC(screen);
                let bitmap = CreateCompatibleBitmap(screen, w, h);
                let old = SelectObject(mem, bitmap);

                // Ask the window to draw itself, DirectX content included.
                let mut frame = if PrintWindow(hwnd, mem, PW_CLIENTONLY | PW_RENDERFULLCONTENT) != 0
                {
                    read_bitmap(mem, bitmap, w, h)
                } else {
                    None
                };
                // Some swap chains don't answer PrintWindow. Copying from the
                // screen is safe only when nothing can be covering the game.
                if frame.as_ref().is_none_or(super::looks_black)
                    && GetForegroundWindow() == hwnd
                    && BitBlt(mem, 0, 0, w, h, screen, ox, oy, SRCCOPY) != 0
                {
                    frame = read_bitmap(mem, bitmap, w, h);
                }

                SelectObject(mem, old);
                DeleteObject(bitmap);
                DeleteDC(mem);
                ReleaseDC(std::ptr::null_mut(), screen);

                match frame {
                    Some(f) if !super::looks_black(&f) => Ok(f),
                    _ => bail!(
                        "the game window came back black. Expected on a loading screen; \
                         otherwise bring Elite to the foreground and use borderless or \
                         windowed mode, not exclusive fullscreen"
                    ),
                }
            }
        }
    }

    /// Copy a 32-bit bitmap out as top-down RGBA.
    unsafe fn read_bitmap(
        dc: HDC,
        bitmap: *mut core::ffi::c_void,
        w: i32,
        h: i32,
    ) -> Option<RgbaImage> {
        // SAFETY: zeroed BITMAPINFO is a valid starting value.
        let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            // Negative height: rows top to bottom.
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..info.bmiHeader
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        // SAFETY: buffer holds exactly h rows of w 32-bit pixels.
        let rows = unsafe {
            GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                buf.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if rows != h {
            return None;
        }
        // GDI hands back BGRA with an undefined alpha byte.
        for px in buf.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            px[3] = 255;
        }
        RgbaImage::from_raw(w as u32, h as u32, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn frame(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, _| Rgba([(x % 256) as u8, 128, 64, 255]))
    }

    #[test]
    fn downscales_to_max_edge_keeping_aspect() {
        let shot = render(&frame(3440, 1440), Region::FULL, 1568, Format::Png, 85).unwrap();
        assert_eq!((shot.width, shot.height), (1568, 656));
        assert_eq!((shot.window_width, shot.window_height), (3440, 1440));
        assert!(shot.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn crops_before_scaling() {
        let left = Region::builtin("left").unwrap();
        let shot = render(&frame(2000, 1000), left, 4096, Format::Jpeg, 85).unwrap();
        assert_eq!((shot.width, shot.height), (1000, 1000));
        assert!(shot.bytes.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn small_crops_are_not_upscaled() {
        let r = Region {
            x: 0.9,
            y: 0.9,
            w: 0.1,
            h: 0.1,
        };
        let shot = render(&frame(1920, 1080), r, 1568, Format::Png, 85).unwrap();
        assert_eq!((shot.width, shot.height), (192, 108));
    }

    #[test]
    fn rejects_regions_outside_the_window() {
        for r in [
            Region {
                x: 0.8,
                y: 0.0,
                w: 0.3,
                h: 1.0,
            },
            Region {
                x: -0.1,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
            Region {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.5,
            },
        ] {
            assert!(render(&frame(100, 100), r, 1568, Format::Png, 85).is_err());
        }
    }

    #[test]
    fn every_builtin_is_valid() {
        for name in Region::BUILTIN_NAMES {
            Region::builtin(name).unwrap().validate().unwrap();
        }
    }

    #[test]
    fn measures_how_much_changed() {
        let a = frame(200, 100);
        assert_eq!(changed_fraction(&a, &a), 0.0);
        let mut b = a.clone();
        for y in 0..50 {
            for x in 0..200 {
                b.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let half = changed_fraction(&a, &b);
        assert!((0.45..=0.55).contains(&half), "{half}");
        assert_eq!(changed_fraction(&a, &frame(100, 100)), 1.0);
    }

    #[test]
    fn detects_black_frames() {
        assert!(looks_black(&RgbaImage::new(64, 64)));
        assert!(!looks_black(&frame(64, 64)));
    }
}
