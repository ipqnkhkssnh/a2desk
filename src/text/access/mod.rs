//! 各平台无障碍树查找

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        mod win;
        pub use win::find_accessible_text;
    } else if #[cfg(target_os = "macos")] {
        mod macos;
        pub use macos::find_accessible_text;
    } else if #[cfg(target_os = "linux")] {
        mod linux;
        pub use linux::find_accessible_text;
    } else {
        use crate::error::{DeskError, DeskResult};
        use crate::types::TextMatch;
        use crate::windows::WindowSelector;

        pub fn find_accessible_text(
            _query: &str,
            _window: Option<&WindowSelector>,
            _limit: usize,
        ) -> DeskResult<Vec<TextMatch>> {
            Err(DeskError::TextFind(
                "当前平台不支持无障碍文本查找".into(),
            ))
        }
    }
}
