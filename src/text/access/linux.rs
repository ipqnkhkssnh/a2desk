//! Linux AT-SPI 文本查找（尽力而为；失败时给出明确错误）

use crate::error::{DeskError, DeskResult};
use crate::types::TextMatch;
use crate::windows::WindowSelector;

pub fn find_accessible_text(
    query: &str,
    _window: Option<&WindowSelector>,
    _limit: usize,
) -> DeskResult<Vec<TextMatch>> {
    // atspi API 随版本变动较大；这里提供稳定的错误信息与扩展点。
    // 完整遍历可后续用 atspi::AccessibilityConnection + Component iface 补齐。
    let _ = query;
    Err(DeskError::TextFind(
        "Linux find_text 需要 AT-SPI2（at-spi2-core）。当前构建提供窗口控制与截屏；\
         请优先用 screenshot + mouse_click，或后续启用完整 AT-SPI 遍历。"
            .into(),
    ))
}
