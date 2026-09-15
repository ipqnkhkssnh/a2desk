//! macOS Accessibility 文本查找（System Events / AX，经 osascript 兜底枚举有限）

use std::process::Command;

use crate::error::{DeskError, DeskResult};
use crate::types::TextMatch;
use crate::windows::{resolve_window, WindowSelector};

pub fn find_accessible_text(
    query: &str,
    window: Option<&WindowSelector>,
    limit: usize,
) -> DeskResult<Vec<TextMatch>> {
    let needle = query.replace('\\', "\\\\").replace('"', "\\\"");
    let app_filter = if let Some(sel) = window {
        let win = resolve_window(sel)?;
        let name = if !win.app_name.is_empty() {
            win.app_name.clone()
        } else {
            win.title.clone()
        };
        format!(
            "set targetProcs to (every process whose name contains \"{}\")",
            name.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        "set targetProcs to (every process whose background only is false)".into()
    };

    let script = format!(
        r#"tell application "System Events"
  {app_filter}
  set out to {{}}
  repeat with p in targetProcs
    try
      set els to (every UI element of p whose name contains "{needle}")
      repeat with e in els
        try
          set nm to name of e
          set pos to position of e
          set sz to size of e
          set end of out to (nm & tab & (item 1 of pos as text) & tab & (item 2 of pos as text) & tab & (item 1 of sz as text) & tab & (item 2 of sz as text))
          if (count of out) ≥ {limit} then exit repeat
        end try
      end repeat
    end try
    if (count of out) ≥ {limit} then exit repeat
  end repeat
  set AppleScript's text item delimiters to linefeed
  return out as text
end tell"#
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| DeskError::TextFind(format!("执行 osascript 失败：{e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(DeskError::TextFind(format!(
            "macOS 无障碍查找失败：{err}（请确认已授予辅助功能权限）"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut matches = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }
        let text = parts[0].to_string();
        let x: i32 = parts[1].parse().unwrap_or(0);
        let y: i32 = parts[2].parse().unwrap_or(0);
        let w: u32 = parts[3].parse().unwrap_or(0);
        let h: u32 = parts[4].parse().unwrap_or(0);
        if w == 0 || h == 0 {
            continue;
        }
        matches.push(TextMatch {
            text,
            global_x: x,
            global_y: y,
            width: w,
            height: h,
            center_x: x + w as i32 / 2,
            center_y: y + h as i32 / 2,
            screen_index: None,
            local_x: None,
            local_y: None,
            source: "accessibility".into(),
        });
        if matches.len() >= limit {
            break;
        }
    }

    if matches.is_empty() {
        return Err(DeskError::TextFind(format!(
            "未找到包含 `{query}` 的无障碍文本"
        )));
    }
    Ok(matches)
}
