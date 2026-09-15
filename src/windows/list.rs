//! 窗口枚举、选择与等待

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use sysinfo::System;
use xcap::Window;

use crate::error::{DeskError, DeskResult};
use crate::screens;
use crate::types::{VisibleBounds, WindowInfo};

/// 窗口选择器。
///
/// * `id` / `pid` / `focused`：精确条件（AND）
/// * `title` / `app_name`：各自可用 `|` 分隔多关键字（字段内 OR）
/// * `query`：对 title / app_name / process_name **任一**命中即可（OR，可用 `|`）
#[derive(Debug, Clone, Default)]
pub struct WindowSelector {
    pub id: Option<u32>,
    pub title: Option<String>,
    pub pid: Option<u32>,
    pub app_name: Option<String>,
    /// 模糊查询：匹配标题 / 应用名 / 进程名（`|` = OR）
    pub query: Option<String>,
    /// true = 只要当前焦点窗口
    pub focused: bool,
}

impl WindowSelector {
    pub fn is_empty(&self) -> bool {
        self.id.is_none()
            && self
                .title
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            && self.pid.is_none()
            && self
                .app_name
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            && self
                .query
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            && !self.focused
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(id) = self.id {
            parts.push(format!("id={id}"));
        }
        if let Some(t) = self.title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("title≈{t}"));
        }
        if let Some(pid) = self.pid {
            parts.push(format!("pid={pid}"));
        }
        if let Some(a) = self
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("app≈{a}"));
        }
        if let Some(q) = self.query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("query≈{q}"));
        }
        if self.focused {
            parts.push("focused".into());
        }
        if parts.is_empty() {
            "（空选择器）".into()
        } else {
            parts.join(", ")
        }
    }
}

/// 列表查询
#[derive(Debug, Clone)]
pub struct WindowQuery {
    pub filter: Option<String>,
    pub pid: Option<u32>,
    pub screen_index: Option<usize>,
    pub only_visible: bool,
    /// 过滤宽×高小于该值的窗口（0=不过滤）
    pub min_area: u32,
    pub sort_by: WindowSort,
    pub limit: usize,
}

impl Default for WindowQuery {
    fn default() -> Self {
        Self {
            filter: None,
            pid: None,
            screen_index: None,
            only_visible: false,
            min_area: 100,
            sort_by: WindowSort::Z,
            limit: 200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSort {
    Z,
    Title,
    Pid,
}

impl WindowSort {
    pub fn parse(s: &str) -> DeskResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "z" | "z_order" | "front" => Ok(WindowSort::Z),
            "title" | "name" => Ok(WindowSort::Title),
            "pid" => Ok(WindowSort::Pid),
            other => Err(DeskError::InvalidArgument(format!(
                "不支持的窗口排序 `{other}`，可选：z、title、pid"
            ))),
        }
    }
}

/// 枚举全部顶层窗口（扁平列表）
pub fn list_windows(q: &WindowQuery) -> DeskResult<Vec<WindowInfo>> {
    let mut list = collect_all_windows()?;
    if let Some(pid) = q.pid {
        list.retain(|w| w.pid == pid);
    }
    if let Some(idx) = q.screen_index {
        list.retain(|w| w.screen_index == Some(idx));
    }
    if q.only_visible {
        list.retain(|w| {
            !w.is_minimized
                && w.width > 1
                && w.height > 1
                && w.visible_bounds
                    .as_ref()
                    .map(|b| b.width > 0 && b.height > 0)
                    .unwrap_or(true)
        });
    }
    if q.min_area > 0 {
        list.retain(|w| {
            let area = (w.width as u64).saturating_mul(w.height as u64);
            area >= q.min_area as u64
        });
    }
    if let Some(filter) = q.filter.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        let needles = split_needles(filter);
        if !needles.is_empty() {
            list.retain(|w| needles.iter().any(|n| window_fuzzy_hit(w, n)));
        }
    }

    match q.sort_by {
        WindowSort::Z => list.sort_by_key(|w| std::cmp::Reverse(w.z)),
        WindowSort::Title => list.sort_by_key(|w| w.title.to_lowercase()),
        WindowSort::Pid => list.sort_by_key(|w| w.pid),
    }

    let limit = if q.limit == 0 {
        2000
    } else {
        q.limit.clamp(1, 2000)
    };
    list.truncate(limit);
    Ok(list)
}

/// 按选择器解析唯一窗口；多个匹配时优先 focused，再按 z 最大
pub fn resolve_window(sel: &WindowSelector) -> DeskResult<WindowInfo> {
    if sel.is_empty() {
        return Err(DeskError::InvalidArgument(
            "请至少提供 id / title / pid / app_name / query / focused 之一来选择窗口".into(),
        ));
    }
    let all = collect_all_windows()?;
    let mut matched: Vec<WindowInfo> = all
        .iter()
        .filter(|w| matches_selector(w, sel))
        .cloned()
        .collect();
    if matched.is_empty() {
        let candidates = nearby_candidates(&all, sel, 8);
        return Err(DeskError::WindowNotFound(format!(
            "没有匹配的窗口（{}）。相近窗口：{}",
            sel.describe(),
            format_candidates(&candidates)
        )));
    }
    matched.sort_by_key(|w| {
        (
            std::cmp::Reverse(w.is_focused),
            // 优先更大的主窗口，避免命中 16×16 辅助窗
            std::cmp::Reverse((w.width as u64).saturating_mul(w.height as u64)),
            std::cmp::Reverse(w.z),
        )
    });
    Ok(matched.remove(0))
}

fn matches_selector(w: &WindowInfo, sel: &WindowSelector) -> bool {
    if let Some(id) = sel.id {
        if w.id != id {
            return false;
        }
    }
    if let Some(pid) = sel.pid {
        if w.pid != pid {
            return false;
        }
    }
    if let Some(t) = sel.title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let needles = split_needles(t);
        if !needles
            .iter()
            .any(|n| w.title.to_lowercase().contains(n))
        {
            return false;
        }
    }
    if let Some(a) = sel
        .app_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let needles = split_needles(a);
        if !needles
            .iter()
            .any(|n| w.app_name.to_lowercase().contains(n))
        {
            return false;
        }
    }
    if let Some(q) = sel.query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let needles = split_needles(q);
        if !needles.iter().any(|n| window_fuzzy_hit(w, n)) {
            return false;
        }
    }
    if sel.focused && !w.is_focused {
        return false;
    }
    true
}

fn split_needles(s: &str) -> Vec<String> {
    s.split('|')
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect()
}

fn window_fuzzy_hit(w: &WindowInfo, needle: &str) -> bool {
    if w.title.to_lowercase().contains(needle) {
        return true;
    }
    if w.app_name.to_lowercase().contains(needle) {
        return true;
    }
    if let Some(pn) = &w.process_name {
        if pn.to_lowercase().contains(needle) {
            return true;
        }
    }
    w.id.to_string() == needle || w.pid.to_string() == needle
}

/// 等待匹配窗口出现；超时附带相近窗口列表
pub fn wait_for_window(sel: &WindowSelector, timeout_ms: u64, interval_ms: u64) -> DeskResult<WindowInfo> {
    let timeout = Duration::from_millis(timeout_ms.clamp(100, 120_000));
    let interval = Duration::from_millis(interval_ms.clamp(50, 5_000));
    let start = Instant::now();
    loop {
        match resolve_window(sel) {
            Ok(w) => return Ok(w),
            Err(DeskError::WindowNotFound(_)) => {}
            Err(e) => return Err(e),
        }
        if start.elapsed() >= timeout {
            let all = collect_all_windows().unwrap_or_default();
            let candidates = nearby_candidates(&all, sel, 10);
            return Err(DeskError::Timeout(format!(
                "等待窗口超时（{}ms）：{}。相近窗口：{}",
                timeout.as_millis(),
                sel.describe(),
                format_candidates(&candidates)
            )));
        }
        std::thread::sleep(interval);
    }
}

fn nearby_candidates(all: &[WindowInfo], sel: &WindowSelector, limit: usize) -> Vec<WindowInfo> {
    let mut hints: Vec<String> = Vec::new();
    for s in [
        sel.query.as_deref(),
        sel.title.as_deref(),
        sel.app_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        hints.extend(split_needles(s));
    }
    let mut scored: Vec<(i32, &WindowInfo)> = all
        .iter()
        .filter(|w| w.width > 16 && w.height > 16)
        .map(|w| {
            let score = hints.iter().map(|h| {
                let mut s = 0i32;
                if w.title.to_lowercase().contains(h) {
                    s += 3;
                }
                if w.app_name.to_lowercase().contains(h) {
                    s += 2;
                }
                if w.process_name
                    .as_ref()
                    .map(|p| p.to_lowercase().contains(h))
                    .unwrap_or(false)
                {
                    s += 2;
                }
                s
            }).sum();
            (score, w)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.z.cmp(&a.1.z)));
    if scored.iter().any(|(s, _)| *s > 0) {
        scored.retain(|(s, _)| *s > 0);
    }
    scored
        .into_iter()
        .take(limit)
        .map(|(_, w)| w.clone())
        .collect()
}

fn format_candidates(list: &[WindowInfo]) -> String {
    if list.is_empty() {
        return "（无）".into();
    }
    let parts: Vec<String> = list
        .iter()
        .map(|w| {
            format!(
                "{{id={}, pid={}, title=\"{}\", app=\"{}\", process={:?}, {}x{}}}",
                w.id,
                w.pid,
                w.title,
                w.app_name,
                w.process_name,
                w.width,
                w.height
            )
        })
        .collect();
    parts.join("; ")
}

/// 按 pid 分组（供 list_apps 复用）
pub fn collect_windows_by_pid() -> DeskResult<HashMap<u32, Vec<WindowInfo>>> {
    let mut map: HashMap<u32, Vec<WindowInfo>> = HashMap::new();
    for w in collect_all_windows()? {
        map.entry(w.pid).or_default().push(w);
    }
    for list in map.values_mut() {
        list.sort_by_key(|w| w.z);
    }
    Ok(map)
}

fn collect_all_windows() -> DeskResult<Vec<WindowInfo>> {
    let screens = screens::all_screens().unwrap_or_default();
    let screen_ids: Vec<(u32, usize)> = screens.iter().map(|s| (s.info.id, s.index)).collect();
    let process_names = process_name_map();

    let windows = std::panic::catch_unwind(AssertUnwindSafe(Window::all))
        .map_err(|_| DeskError::SystemInfo("枚举窗口时发生内部错误".into()))?
        .map_err(|e| DeskError::SystemInfo(format!("枚举窗口失败：{e}")))?;

    let mut out = Vec::with_capacity(windows.len());
    for w in windows {
        let Ok(pid) = w.pid() else { continue };
        let id = w.id().unwrap_or(0);
        if id == 0 {
            continue;
        }
        let x = w.x().unwrap_or(0);
        let y = w.y().unwrap_or(0);
        let width = w.width().unwrap_or(0);
        let height = w.height().unwrap_or(0);

        let monitor_index = w
            .current_monitor()
            .ok()
            .and_then(|m| m.id().ok())
            .and_then(|mid| {
                screen_ids
                    .iter()
                    .find(|(sid, _)| *sid == mid)
                    .map(|(_, idx)| *idx)
            })
            .or_else(|| infer_screen_index(&screens, x, y, width, height));

        let visible_bounds = monitor_index.and_then(|idx| {
            screens.get(idx).map(|s| {
                let sx = s.info.x;
                let sy = s.info.y;
                let sw = s.info.width as i32;
                let sh = s.info.height as i32;
                let left = x.max(sx);
                let top = y.max(sy);
                let right = (x + width as i32).min(sx + sw);
                let bottom = (y + height as i32).min(sy + sh);
                let vw = (right - left).max(0) as u32;
                let vh = (bottom - top).max(0) as u32;
                VisibleBounds {
                    x: left - sx,
                    y: top - sy,
                    width: vw,
                    height: vh,
                }
            })
        });

        out.push(WindowInfo {
            id,
            pid,
            process_name: process_names.get(&pid).cloned(),
            title: w.title().unwrap_or_default(),
            app_name: w.app_name().unwrap_or_default(),
            x,
            y,
            width,
            height,
            z: w.z().unwrap_or(0),
            is_minimized: w.is_minimized().unwrap_or(false),
            is_maximized: w.is_maximized().unwrap_or(false),
            is_focused: w.is_focused().unwrap_or(false),
            screen_index: monitor_index,
            visible_bounds,
        });
    }
    Ok(out)
}

fn process_name_map() -> HashMap<u32, String> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32(), p.name().to_string_lossy().into_owned()))
        .collect()
}

fn infer_screen_index(
    screens: &[screens::Screen],
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<usize> {
    let cx = x + (width as i32) / 2;
    let cy = y + (height as i32) / 2;
    screens.iter().find_map(|s| {
        let w = s.info.width as i32;
        let h = s.info.height as i32;
        if cx >= s.info.x && cx < s.info.x + w && cy >= s.info.y && cy < s.info.y + h {
            Some(s.index)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(title: &str, app: &str, process: &str) -> WindowInfo {
        WindowInfo {
            id: 1,
            pid: 2,
            process_name: Some(process.into()),
            title: title.into(),
            app_name: app.into(),
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            z: 1,
            is_minimized: false,
            is_maximized: false,
            is_focused: false,
            screen_index: Some(0),
            visible_bounds: None,
        }
    }

    #[test]
    fn query_matches_app_or_title() {
        let w = sample("优优", "YouYou", "agent-app.exe");
        let sel = WindowSelector {
            query: Some("YouYou|优优".into()),
            ..Default::default()
        };
        assert!(matches_selector(&w, &sel));
        let sel2 = WindowSelector {
            query: Some("agent-app".into()),
            ..Default::default()
        };
        assert!(matches_selector(&w, &sel2));
    }

    #[test]
    fn title_pipe_or() {
        let w = sample("优优助手", "YouYou", "agent-app.exe");
        let sel = WindowSelector {
            title: Some("YouYou|优优".into()),
            ..Default::default()
        };
        assert!(matches_selector(&w, &sel));
    }
}
