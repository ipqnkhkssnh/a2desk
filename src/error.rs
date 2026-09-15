use rmcp::model::{CallToolResult, ContentBlock};

/// a2desk 统一错误类型。
///
/// 工具层会把错误转换成 `CallToolResult{isError:true}`，
/// 这样模型能直接读到人类可读的失败原因，而不是一个 JSON-RPC 协议错误。
#[derive(Debug, thiserror::Error)]
pub enum DeskError {
    #[error("找不到指定的屏幕：{0}")]
    ScreenNotFound(String),

    #[error("找不到指定的窗口：{0}")]
    WindowNotFound(String),

    #[error("参数错误：{0}")]
    InvalidArgument(String),

    #[error("截屏失败：{0}")]
    Capture(String),

    #[error("鼠标/键盘输入失败：{0}")]
    Input(String),

    #[error("获取系统信息失败：{0}")]
    SystemInfo(String),

    #[error("窗口操作失败：{0}")]
    WindowOp(String),

    #[error("剪贴板操作失败：{0}")]
    Clipboard(String),

    #[error("文本查找失败：{0}")]
    TextFind(String),

    #[error("等待超时：{0}")]
    Timeout(String),

    #[error("输入控制线程已退出，无法执行输入操作")]
    InputWorkerGone,
}

pub type DeskResult<T> = std::result::Result<T, DeskError>;

impl DeskError {
    /// 转成 MCP 工具级错误结果
    pub fn into_tool_error(self) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(self.to_string())])
    }

    /// 附加一段排查提示
    pub fn with_hint(self, hint: impl AsRef<str>) -> Self {
        let hint = hint.as_ref();
        if hint.is_empty() {
            return self;
        }
        match self {
            DeskError::Input(m) => DeskError::Input(format!("{m}\n提示：{hint}")),
            DeskError::Capture(m) => DeskError::Capture(format!("{m}\n提示：{hint}")),
            DeskError::WindowOp(m) => DeskError::WindowOp(format!("{m}\n提示：{hint}")),
            DeskError::TextFind(m) => DeskError::TextFind(format!("{m}\n提示：{hint}")),
            other => other,
        }
    }
}

/// 工具的返回类型：Ok 是成功结果，Err 是工具级错误结果。
pub type ToolResult = std::result::Result<CallToolResult, CallToolResult>;

/// 把 `DeskResult<CallToolResult>` 收敛成 `ToolResult`
#[macro_export]
macro_rules! tool_try {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Err(e.into_tool_error()),
        }
    };
}
