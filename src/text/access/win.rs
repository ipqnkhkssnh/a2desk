//! Windows UI Automation 文本查找

use uiautomation::controls::ControlType;
use uiautomation::types::UIProperty;
use uiautomation::variants::Variant;
use uiautomation::{UIAutomation, UIElement};

use crate::error::{DeskError, DeskResult};
use crate::types::TextMatch;
use crate::windows::{resolve_window, WindowSelector};

pub fn find_accessible_text(
    query: &str,
    window: Option<&WindowSelector>,
    limit: usize,
) -> DeskResult<Vec<TextMatch>> {
    let automation = UIAutomation::new()
        .map_err(|e| DeskError::TextFind(format!("初始化 UI Automation 失败：{e}")))?;

    let root = if let Some(sel) = window {
        let win = resolve_window(sel)?;
        let hwnd = win.id as isize;
        automation
            .element_from_handle(hwnd.into())
            .map_err(|e| DeskError::TextFind(format!("绑定窗口无障碍树失败：{e}")))?
    } else {
        automation
            .get_root_element()
            .map_err(|e| DeskError::TextFind(format!("获取桌面根元素失败：{e}")))?
    };

    let needle = query.to_lowercase();
    let mut out = Vec::new();
    walk(&automation, &root, &needle, limit, &mut out)?;
    Ok(out)
}

fn walk(
    automation: &UIAutomation,
    element: &UIElement,
    needle: &str,
    limit: usize,
    out: &mut Vec<TextMatch>,
) -> DeskResult<()> {
    if out.len() >= limit {
        return Ok(());
    }

    if let Some(m) = match_element(element, needle) {
        out.push(m);
        if out.len() >= limit {
            return Ok(());
        }
    }

    let walker = automation
        .get_control_view_walker()
        .map_err(|e| DeskError::TextFind(format!("创建 TreeWalker 失败：{e}")))?;

    let mut child = walker.get_first_child(element).ok();
    while let Some(ref c) = child {
        walk(automation, c, needle, limit, out)?;
        if out.len() >= limit {
            break;
        }
        child = walker.get_next_sibling(c).ok();
    }
    Ok(())
}

fn match_element(element: &UIElement, needle: &str) -> Option<TextMatch> {
    let name = element.get_name().unwrap_or_default();
    let value = element
        .get_property_value(UIProperty::ValueValue)
        .ok()
        .and_then(|v: Variant| v.get_string().ok())
        .unwrap_or_default();

    let text = if name.to_lowercase().contains(needle) {
        name.clone()
    } else if value.to_lowercase().contains(needle) {
        value
    } else {
        return None;
    };

    // 跳过无尺寸或过大的容器（桌面本身）
    let rect = element.get_bounding_rectangle().ok()?;
    let width = (rect.get_right() - rect.get_left()).max(0) as u32;
    let height = (rect.get_bottom() - rect.get_top()).max(0) as u32;
    if width == 0 || height == 0 || width > 8000 || height > 8000 {
        // 仍允许按钮等小控件；过滤巨大根节点
        if width > 4000 && height > 2000 {
            return None;
        }
        if width == 0 || height == 0 {
            return None;
        }
    }

    // 优先控件类型为文本/按钮等
    let _ = element.get_control_type().unwrap_or(ControlType::Custom);

    let x = rect.get_left();
    let y = rect.get_top();
    Some(TextMatch {
        text,
        global_x: x,
        global_y: y,
        width,
        height,
        center_x: x + width as i32 / 2,
        center_y: y + height as i32 / 2,
        screen_index: None,
        local_x: None,
        local_y: None,
        source: "accessibility".into(),
    })
}
