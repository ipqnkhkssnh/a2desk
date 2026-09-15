//! 屏幕/窗口内文本查找（无障碍树优先）

mod access;

use std::time::{Duration, Instant};

use crate::error::{DeskError, DeskResult};
use crate::screens;
use crate::types::TextMatch;
use crate::windows::WindowSelector;

/// 查找文本（子串，大小写不敏感）
pub fn find_text(
    query: &str,
    window: Option<&WindowSelector>,
    limit: usize,
) -> DeskResult<Vec<TextMatch>> {
    let q = query.trim();
    if q.is_empty() {
        return Err(DeskError::InvalidArgument("query 不能为空".into()));
    }
    let limit = if limit == 0 { 50 } else { limit.clamp(1, 200) };

    let mut matches = access::find_accessible_text(q, window, limit)?;
    enrich_screen_coords(&mut matches);
    Ok(matches)
}

/// 等待文本出现
pub fn wait_for_text(
    query: &str,
    window: Option<&WindowSelector>,
    timeout_ms: u64,
    poll_ms: u64,
) -> DeskResult<TextMatch> {
    let timeout = Duration::from_millis(timeout_ms.clamp(100, 120_000));
    let poll = Duration::from_millis(poll_ms.clamp(50, 5_000));
    let start = Instant::now();
    loop {
        match find_text(query, window, 1) {
            Ok(list) if !list.is_empty() => return Ok(list.into_iter().next().unwrap()),
            Ok(_) | Err(DeskError::TextFind(_)) => {}
            Err(e) => return Err(e),
        }
        if start.elapsed() >= timeout {
            return Err(DeskError::Timeout(format!(
                "等待文本超时（{timeout_ms}ms）：`{query}`"
            )));
        }
        std::thread::sleep(poll);
    }
}

fn enrich_screen_coords(matches: &mut [TextMatch]) {
    let screens = screens::all_screens().unwrap_or_default();
    for m in matches.iter_mut() {
        let cx = m.center_x;
        let cy = m.center_y;
        if let Some(s) = screens.iter().find(|s| {
            let w = s.info.width as i32;
            let h = s.info.height as i32;
            cx >= s.info.x && cx < s.info.x + w && cy >= s.info.y && cy < s.info.y + h
        }) {
            m.screen_index = Some(s.index);
            m.local_x = Some(cx - s.info.x);
            m.local_y = Some(cy - s.info.y);
        }
    }
}
