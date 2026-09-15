//! 窗口枚举与控制（焦点 / 移动 / 缩放 / 显隐 / 关闭 / 截窗）

mod control;
mod list;

pub use control::{
    close_window, focus_window, maximize_window, minimize_window, move_window, resize_window,
    restore_window, set_window_bounds, set_window_screen,
};
pub use list::{
    collect_windows_by_pid, list_windows, resolve_window, wait_for_window, WindowQuery,
    WindowSelector, WindowSort,
};

use crate::error::{DeskError, DeskResult};
use crate::screens::{self, CaptureOptions, OutFormat};
use crate::types::{CaptureMeta, WindowInfo};

/// 按窗口截图：优先 xcap::Window::capture_image，失败则按窗口矩形裁剪屏幕
pub fn screenshot_window(
    sel: &WindowSelector,
    opts: &CaptureOptions,
) -> DeskResult<(Vec<u8>, CaptureMeta, WindowInfo)> {
    let win = resolve_window(sel)?;
    let raw = capture_window_rgba(&win)?;
    let (bytes, meta) = screens::encode_rgba(&raw, opts)?;
    Ok((bytes, meta, win))
}

fn capture_window_rgba(win: &WindowInfo) -> DeskResult<image::RgbaImage> {
    use std::panic::AssertUnwindSafe;
    use xcap::Window;

    let windows = std::panic::catch_unwind(AssertUnwindSafe(Window::all))
        .map_err(|_| DeskError::Capture("枚举窗口时发生内部错误".into()))?
        .map_err(|e| DeskError::Capture(format!("枚举窗口失败：{e}")))?;

    for w in windows {
        let id = w.id().unwrap_or(0);
        if id != win.id {
            continue;
        }
        if let Ok(img) = w.capture_image() {
            return Ok(img);
        }
        break;
    }

    // 回退：按窗口矩形从所在屏幕裁剪
    let screen = match win.screen_index {
        Some(idx) => screens::resolve_screen(Some(&idx.to_string()))?,
        None => screens::resolve_screen(None)?,
    };
    let local_x = (win.x - screen.info.x) as i64;
    let local_y = (win.y - screen.info.y) as i64;
    let sw = screen.info.width as i64;
    let sh = screen.info.height as i64;
    let x = local_x.clamp(0, (sw - 1).max(0));
    let y = local_y.clamp(0, (sh - 1).max(0));
    let w = (win.width as i64).clamp(1, (sw - x).max(1));
    let h = (win.height as i64).clamp(1, (sh - y).max(1));
    let _ = OutFormat::Png;
    screen
        .monitor
        .capture_region(x as u32, y as u32, w as u32, h as u32)
        .map_err(|e| DeskError::Capture(format!("按窗口矩形裁剪屏幕失败：{e}")))
}

/// 从 MCP 参数拼装窗口选择器
pub fn selector_from_params(
    id: Option<u32>,
    title: Option<String>,
    pid: Option<u32>,
    app_name: Option<String>,
    query: Option<String>,
    focused: Option<bool>,
) -> WindowSelector {
    WindowSelector {
        id,
        title,
        pid,
        app_name,
        query,
        focused: focused.unwrap_or(false),
    }
}
