//! 跨平台窗口控制分发

use crate::error::{DeskError, DeskResult};
use crate::screens;
use crate::types::WindowInfo;
use crate::windows::list::{resolve_window, WindowSelector};

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        mod win32;
        use win32 as platform;
    } else if #[cfg(target_os = "macos")] {
        mod macos;
        use macos as platform;
    } else if #[cfg(target_os = "linux")] {
        mod linux_x11;
        use linux_x11 as platform;
    } else {
        compile_error!("a2desk 窗口控制仅支持 Windows / macOS / Linux(X11)");
    }
}

pub fn focus_window(sel: &WindowSelector) -> DeskResult<(WindowInfo, bool)> {
    let win = resolve_window(sel)?;
    let was_minimized = win.is_minimized || platform::is_minimized(win.id);
    if was_minimized {
        let _ = platform::restore(win.id);
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    platform::focus(win.id, win.pid)?;
    // 再短等，让前台切换生效
    std::thread::sleep(std::time::Duration::from_millis(50));
    let refreshed = resolve_window(&WindowSelector {
        id: Some(win.id),
        ..Default::default()
    })
    .unwrap_or(win);
    Ok((refreshed, was_minimized))
}

pub fn move_window(sel: &WindowSelector, x: i32, y: i32) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    platform::set_bounds(win.id, x, y, win.width.max(1), win.height.max(1))?;
    refresh(win.id)
}

pub fn resize_window(sel: &WindowSelector, width: u32, height: u32) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    if width == 0 || height == 0 {
        return Err(DeskError::InvalidArgument(
            "width/height 必须大于 0".into(),
        ));
    }
    platform::set_bounds(win.id, win.x, win.y, width, height)?;
    refresh(win.id)
}

pub fn set_window_bounds(
    sel: &WindowSelector,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    if width == 0 || height == 0 {
        return Err(DeskError::InvalidArgument(
            "width/height 必须大于 0".into(),
        ));
    }
    platform::set_bounds(win.id, x, y, width, height)?;
    refresh(win.id)
}

/// 把窗口放到指定屏幕（保留尺寸，默认落在该屏左上角偏移 40,40；过大则缩放适配）
pub fn set_window_screen(
    sel: &WindowSelector,
    screen: Option<&str>,
    margin: i32,
) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    let screen = screens::resolve_screen(screen)?;
    let margin = margin.clamp(0, 200);
    let max_w = screen.info.width.saturating_sub(margin as u32 * 2).max(100);
    let max_h = screen.info.height.saturating_sub(margin as u32 * 2).max(100);
    let width = win.width.clamp(100, max_w);
    let height = win.height.clamp(100, max_h);
    let x = screen.info.x + margin;
    let y = screen.info.y + margin;
    platform::restore(win.id).ok();
    platform::set_bounds(win.id, x, y, width, height)?;
    platform::focus(win.id, win.pid).ok();
    refresh(win.id)
}

pub fn minimize_window(sel: &WindowSelector) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    platform::minimize(win.id)?;
    refresh(win.id)
}

pub fn maximize_window(sel: &WindowSelector) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    platform::maximize(win.id)?;
    refresh(win.id)
}

pub fn restore_window(sel: &WindowSelector) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    platform::restore(win.id)?;
    refresh(win.id)
}

pub fn close_window(sel: &WindowSelector) -> DeskResult<WindowInfo> {
    let win = resolve_window(sel)?;
    platform::close(win.id)?;
    Ok(win)
}

fn refresh(id: u32) -> DeskResult<WindowInfo> {
    // 给系统一点时间更新窗口状态
    std::thread::sleep(std::time::Duration::from_millis(50));
    resolve_window(&WindowSelector {
        id: Some(id),
        ..Default::default()
    })
}
