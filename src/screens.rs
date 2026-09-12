//! 屏幕枚举与截屏

use std::io::Cursor;

use image::imageops::FilterType;
use image::{ExtendedColorType, ImageFormat as ImgFormat, RgbaImage};
use xcap::Monitor;

use crate::error::{DeskError, DeskResult};
use crate::types::{CaptureMeta, Region, ScreenInfo};

/// 已解析的一块屏幕，附带底层 xcap Monitor
#[derive(Debug, Clone)]
pub struct Screen {
    pub index: usize,
    pub info: ScreenInfo,
    pub monitor: Monitor,
}

/// 输出图片格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Jpeg,
    Png,
}

impl OutFormat {
    pub fn parse(s: &str) -> DeskResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "jpeg" | "jpg" => Ok(OutFormat::Jpeg),
            "png" => Ok(OutFormat::Png),
            other => Err(DeskError::InvalidArgument(format!(
                "不支持的图片格式 `{other}`，可选：jpeg、png"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OutFormat::Jpeg => "jpeg",
            OutFormat::Png => "png",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            OutFormat::Jpeg => "image/jpeg",
            OutFormat::Png => "image/png",
        }
    }
}

fn primary_first(mut screens: Vec<(ScreenInfo, Monitor)>) -> Vec<(ScreenInfo, Monitor)> {
    // 主屏放最前，其余按 (y, x) 排序，保证索引稳定且符合"从左到右、从上到下"的直觉
    screens.sort_by_key(|(info, _)| {
        (
            if info.is_primary { 0i32 } else { 1 },
            info.y,
            info.x,
        )
    });
    screens
}

/// 枚举所有屏幕（索引 0 为主屏）
pub fn all_screens() -> DeskResult<Vec<Screen>> {
    let monitors = Monitor::all()
        .map_err(|e| DeskError::Capture(format!("枚举显示器失败：{e}")))?;

    if monitors.is_empty() {
        return Err(DeskError::Capture("系统未返回任何显示器".into()));
    }

    let mut rows: Vec<(ScreenInfo, Monitor)> = Vec::with_capacity(monitors.len());
    for m in monitors {
        let info = ScreenInfo {
            index: 0, // 排序后再填
            id: m.id().unwrap_or(0),
            name: display_name(&m),
            x: m.x().unwrap_or(0),
            y: m.y().unwrap_or(0),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            is_primary: m.is_primary().unwrap_or(false),
            is_builtin: m.is_builtin().unwrap_or(false),
            scale_factor: m.scale_factor().unwrap_or(1.0),
            rotation: m.rotation().unwrap_or(0.0),
            refresh_rate: m.frequency().unwrap_or(0.0),
        };
        rows.push((info, m));
    }

    let rows = primary_first(rows);
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(index, (mut info, monitor))| {
            info.index = index;
            Screen {
                index,
                info,
                monitor,
            }
        })
        .collect())
}

fn display_name(m: &Monitor) -> String {
    // friendly_name 在 macOS 上会走 AppKit，失败时退回 name
    match m.friendly_name() {
        Ok(n) if !n.trim().is_empty() => n,
        _ => m.name().unwrap_or_else(|_| format!("Display {}", m.id().unwrap_or(0))),
    }
}

/// 解析屏幕选择参数。
///
/// 支持：
/// * `None` / `""` / `primary` / `main` / `主屏` → 主屏
/// * 纯数字 → 屏幕索引（对应 `list_screens` 的 `index`）
/// * 其它 → 屏幕名称的大小写不敏感子串匹配
pub fn resolve_screen(sel: Option<&str>) -> DeskResult<Screen> {
    let screens = all_screens()?;
    let raw = sel.map(str::trim).unwrap_or("");

    if raw.is_empty()
        || raw.eq_ignore_ascii_case("primary")
        || raw.eq_ignore_ascii_case("main")
        || raw.eq_ignore_ascii_case("default")
        || raw == "主屏"
        || raw == "主显示器"
    {
        return screens
            .iter()
            .find(|s| s.info.is_primary)
            .or_else(|| screens.first())
            .cloned()
            .ok_or_else(|| DeskError::ScreenNotFound("没有可用屏幕".into()));
    }

    if let Ok(idx) = raw.parse::<usize>() {
        return screens.get(idx).cloned().ok_or_else(|| {
            DeskError::ScreenNotFound(format!(
                "屏幕索引 {idx} 不存在，{}",
                screen_catalog(&screens)
            ))
        });
    }

    let needle = raw.to_lowercase();
    let matches: Vec<&Screen> = screens
        .iter()
        .filter(|s| s.info.name.to_lowercase().contains(&needle))
        .collect();

    match matches.as_slice() {
        [] => Err(DeskError::ScreenNotFound(format!(
            "没有名称包含 `{raw}` 的屏幕，{}",
            screen_catalog(&screens)
        ))),
        [one] => Ok((*one).clone()),
        many => {
            // 多个匹配时优先主屏
            let chosen = many
                .iter()
                .find(|s| s.info.is_primary)
                .copied()
                .unwrap_or(many[0]);
            Ok(chosen.clone())
        }
    }
}

fn screen_catalog(screens: &[Screen]) -> String {
    let list = screens
        .iter()
        .map(|s| {
            format!(
                "[{}] \"{}\"{} {}x{} @({},{})",
                s.index,
                s.info.name,
                if s.info.is_primary { "(主屏)" } else { "" },
                s.info.width,
                s.info.height,
                s.info.x,
                s.info.y
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("可用屏幕：{list}")
}

/// 截屏参数
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// 区域（屏幕内局部坐标），None = 整屏
    pub region: Option<Region>,
    /// 期望的「图片像素 / 屏幕坐标单位」比例，1.0 表示 1 图片像素 = 1 坐标单位
    pub scale: f64,
    /// 输出图片最大宽度（等比缩放）
    pub max_width: Option<u32>,
    pub format: OutFormat,
    pub quality: u8,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            region: None,
            scale: 1.0,
            max_width: None,
            format: OutFormat::Jpeg,
            quality: 85,
        }
    }
}

/// 截取指定屏幕的指定区域，返回编码后的图片字节与元信息
pub fn capture(screen: &Screen, opts: &CaptureOptions) -> DeskResult<(Vec<u8>, CaptureMeta)> {
    let sw = screen.info.width as i64;
    let sh = screen.info.height as i64;
    if sw <= 0 || sh <= 0 {
        return Err(DeskError::Capture(format!(
            "屏幕 [{}] 尺寸异常：{}x{}",
            screen.index, sw, sh
        )));
    }

    let req = opts.region.unwrap_or(Region {
        x: 0,
        y: 0,
        width: sw as u32,
        height: sh as u32,
    });

    let mut x = req.x;
    let mut y = req.y;
    let mut w = req.width as i64;
    let mut h = req.height as i64;
    if w <= 0 || h <= 0 {
        return Err(DeskError::InvalidArgument(format!(
            "region 的 width/height 必须大于 0，当前为 {}x{}",
            req.width, req.height
        )));
    }
    let clamped = x < 0
        || y < 0
        || x > sw - 1
        || y > sh - 1
        || x + w > sw
        || y + h > sh;
    x = x.clamp(0, sw - 1);
    y = y.clamp(0, sh - 1);
    w = w.clamp(1, sw - x);
    h = h.clamp(1, sh - y);

    let raw: RgbaImage = screen
        .monitor
        .capture_region(x as u32, y as u32, w as u32, h as u32)
        .map_err(|e| {
            DeskError::Capture(format!(
                "捕获屏幕 [{}]{} 失败：{e}",
                screen.index,
                region_desc(x, y, w, h)
            ))
        })?;

    let (image, _) = resize_for_output(&raw, w, h, opts)?;

    let mut buf: Vec<u8> = Vec::new();
    match opts.format {
        OutFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, opts.quality);
            encoder
                .encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    ExtendedColorType::Rgb8,
                )
                .map_err(|e| DeskError::Capture(format!("JPEG 编码失败：{e}")))?;
        }
        OutFormat::Png => {
            image::DynamicImage::ImageRgba8(image.clone())
                .write_to(&mut Cursor::new(&mut buf), ImgFormat::Png)
                .map_err(|e| DeskError::Capture(format!("PNG 编码失败：{e}")))?;
        }
    }

    let pixel_ratio = image.width() as f64 / w as f64;
    let meta = CaptureMeta {
        region: Region {
            x,
            y,
            width: w as u32,
            height: h as u32,
        },
        region_clamped: clamped,
        image_width: image.width(),
        image_height: image.height(),
        pixel_ratio,
        format: opts.format.as_str().to_string(),
        quality: if opts.format == OutFormat::Jpeg {
            opts.quality
        } else {
            0
        },
        byte_size: buf.len(),
    };

    Ok((buf, meta))
}

fn resize_for_output(
    raw: &RgbaImage,
    region_w: i64,
    region_h: i64,
    opts: &CaptureOptions,
) -> DeskResult<(RgbaImage, f64)> {
    let raw_w = raw.width().max(1);
    let raw_h = raw.height().max(1);
    let raw_ratio = raw_w as f64 / region_w as f64;

    let scale = if opts.scale.is_finite() && opts.scale > 0.0 {
        opts.scale
    } else {
        1.0
    };

    let mut target_w = (region_w as f64 * scale).round().max(1.0) as u32;
    if let Some(mw) = opts.max_width {
        if mw > 0 && target_w > mw {
            target_w = mw;
        }
    }
    let target_h = ((target_w as f64) * (raw_h as f64) / (raw_w as f64)).round().max(1.0) as u32;

    let _ = region_h;
    if target_w == raw_w && target_h == raw_h {
        return Ok((raw.clone(), raw_ratio));
    }

    let resized = image::imageops::resize(raw, target_w, target_h, FilterType::Triangle);
    Ok((resized, raw_ratio))
}

fn region_desc(x: i64, y: i64, w: i64, h: i64) -> String {
    format!(" 区域 ({x},{y},{w},{h})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_format() {
        assert_eq!(OutFormat::parse("JPEG").unwrap(), OutFormat::Jpeg);
        assert_eq!(OutFormat::parse("jpg").unwrap(), OutFormat::Jpeg);
        assert_eq!(OutFormat::parse("png").unwrap(), OutFormat::Png);
        assert!(OutFormat::parse("bmp").is_err());
    }
}
