use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 一块屏幕（显示器）的信息。
///
/// `x`/`y` 是该屏幕在虚拟桌面中的左上角坐标，`width`/`height` 是它的逻辑尺寸。
/// 鼠标工具使用的是"屏幕内局部坐标"，需要配合这里的 `index` 使用。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenInfo {
    /// 屏幕索引，鼠标/截屏工具用它来指定屏幕（0 = 主屏）
    pub index: usize,
    /// 系统返回的显示器 ID
    pub id: u32,
    /// 显示器名称
    pub name: String,
    /// 虚拟桌面中的左上角 X
    pub x: i32,
    /// 虚拟桌面中的左上角 Y
    pub y: i32,
    /// 宽度（macOS 为逻辑点，Windows/Linux 为像素）
    pub width: u32,
    /// 高度（macOS 为逻辑点，Windows/Linux 为像素）
    pub height: u32,
    /// 是否为系统主屏
    pub is_primary: bool,
    /// 是否为内建屏幕
    pub is_builtin: bool,
    /// 缩放因子（Retina 屏为 2.0；1.0 表示截屏 1 像素 = 1 坐标单位）
    pub scale_factor: f32,
    /// 旋转角度
    pub rotation: f32,
    /// 刷新率 Hz
    pub refresh_rate: f32,
}

impl ScreenInfo {
    /// 该屏幕右下角（不含）的局部坐标
    pub fn local_max(&self) -> (i64, i64) {
        (self.width as i64 - 1, self.height as i64 - 1)
    }
}

/// 截屏区域：**相对指定屏幕左上角**的局部坐标。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    /// 相对屏幕左上角的 X
    pub x: i64,
    /// 相对屏幕左上角的 Y
    pub y: i64,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
}

/// 截屏结果的元信息（图片本身以 MCP image content 返回）。
#[derive(Debug, Clone, Serialize)]
pub struct CaptureMeta {
    /// 实际使用的屏幕（可能因越界被裁剪）
    pub region: Region,
    /// 请求的区域是否被裁剪过
    pub region_clamped: bool,
    /// 输出图片宽度（像素）
    pub image_width: u32,
    /// 输出图片高度（像素）
    pub image_height: u32,
    /// 图片像素 / 屏幕坐标单位 的比例。
    /// 屏幕坐标 = region.x + 图片X / pixel_ratio
    pub pixel_ratio: f64,
    /// 图像格式
    pub format: String,
    /// 编码质量（JPEG）
    pub quality: u8,
    /// 编码后字节数
    pub byte_size: usize,
}

/// 窗口在所属屏幕上的可见矩形（相对该屏幕左上角的局部坐标）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VisibleBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 窗口信息
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    /// 窗口 ID（Windows≈HWND 截断为 u32；macOS=CGWindowID；Linux=XID）
    pub id: u32,
    /// 所属进程 ID
    pub pid: u32,
    /// 进程名（如 `agent-app.exe` / `Cursor`），枚举时尽力填充
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    /// 窗口标题（macOS 未授予录屏权限时可能为空）
    pub title: String,
    /// 应用名
    pub app_name: String,
    /// 窗口左上角（虚拟桌面坐标）
    pub x: i32,
    /// 窗口左上角（虚拟桌面坐标）
    pub y: i32,
    /// 窗口宽度
    pub width: u32,
    /// 窗口高度
    pub height: u32,
    /// 窗口层级（越大越靠前）
    pub z: i32,
    /// 是否最小化
    pub is_minimized: bool,
    /// 是否最大化
    pub is_maximized: bool,
    /// 是否为当前焦点窗口
    pub is_focused: bool,
    /// 窗口所在屏幕的索引（对应 `list_screens`）
    pub screen_index: Option<usize>,
    /// 相对所属屏幕的可见区域（窗口移出屏幕时会小于完整宽高）
    pub visible_bounds: Option<VisibleBounds>,
}

/// 进程（应用）信息
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// 进程 ID
    pub pid: u32,
    /// 进程名
    pub name: String,
    /// 可执行文件路径
    pub exe: Option<String>,
    /// 启动命令行
    pub cmd: Vec<String>,
    /// 父进程 ID
    pub parent_pid: Option<u32>,
    /// 状态（Run/Sleep/...）
    pub status: String,
    /// CPU 占用百分比（需要一次采样间隔）
    pub cpu_usage: f32,
    /// 内存占用（字节）
    pub memory_bytes: u64,
    /// 启动时间（Unix 秒）
    pub start_time: u64,
    /// 该进程拥有的窗口
    pub windows: Vec<WindowInfo>,
}

/// 文本命中结果（屏幕坐标，便于直接喂给 mouse_*）
#[derive(Debug, Clone, Serialize)]
pub struct TextMatch {
    /// 匹配到的文本
    pub text: String,
    /// 命中框左上角 X（虚拟桌面绝对坐标）
    pub global_x: i32,
    /// 命中框左上角 Y（虚拟桌面绝对坐标）
    pub global_y: i32,
    /// 命中框宽度
    pub width: u32,
    /// 命中框高度
    pub height: u32,
    /// 中心点 X（绝对）
    pub center_x: i32,
    /// 中心点 Y（绝对）
    pub center_y: i32,
    /// 所在屏幕索引
    pub screen_index: Option<usize>,
    /// 相对该屏幕的局部中心 X
    pub local_x: Option<i32>,
    /// 相对该屏幕的局部中心 Y
    pub local_y: Option<i32>,
    /// 来源：accessibility / ocr
    pub source: String,
}
