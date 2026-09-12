//! a2desk —— 跨平台桌面控制 MCP 服务器
//!
//! 提供屏幕信息 / 截屏 / 鼠标 / 键盘 / 应用枚举等 MCP 工具，
//! 支持 Windows、macOS、Linux(X11)。
//!
//! 坐标系约定（所有鼠标工具与截屏 region 使用同一套坐标）：
//! * `list_screens` 返回的 `x/y/width/height` 是每块屏幕在"虚拟桌面"中的位置与尺寸；
//! * 鼠标工具接收的是**屏幕内局部坐标**（相对该屏幕左上角），
//!   内部会加上该屏幕的 `x/y` 转换为全屏绝对坐标；
//! * macOS 上该坐标的单位是逻辑点(pt)，Windows/Linux 上是物理像素。

pub mod apps;
pub mod error;
pub mod input;
pub mod screens;
pub mod selftest;
pub mod server;
pub mod types;

pub const SERVER_NAME: &str = "a2desk";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 运行期配置
#[derive(Debug, Clone)]
pub struct Config {
    /// macOS: 缺少辅助功能权限时是否弹出系统授权提示
    pub prompt_for_permission: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prompt_for_permission: true,
        }
    }
}
