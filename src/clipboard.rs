//! 系统剪贴板读写

use arboard::Clipboard;

use crate::error::{DeskError, DeskResult};

/// 读取剪贴板文本
pub fn get_text() -> DeskResult<String> {
    let mut cb = Clipboard::new().map_err(|e| DeskError::Clipboard(format!("打开剪贴板失败：{e}")))?;
    cb.get_text()
        .map_err(|e| DeskError::Clipboard(format!("读取剪贴板文本失败：{e}")))
}

/// 写入剪贴板文本
pub fn set_text(text: &str) -> DeskResult<()> {
    let mut cb = Clipboard::new().map_err(|e| DeskError::Clipboard(format!("打开剪贴板失败：{e}")))?;
    cb.set_text(text.to_string())
        .map_err(|e| DeskError::Clipboard(format!("写入剪贴板失败：{e}")))
}
