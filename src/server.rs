//! MCP 服务器：把屏幕/鼠标/键盘/应用能力暴露成 MCP 工具

use base64::Engine as _;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::apps::{self, AppQuery, AppSort};
use crate::error::{DeskError, DeskResult, ToolResult};
use crate::input::{parse_key_group, InputHandle, MouseButton, ScrollDir, MAX_SCROLL_AMOUNT};
use crate::screens::{self, CaptureOptions, OutFormat};
use crate::types::Region;
use crate::Config;

/// a2desk MCP 服务器
#[derive(Clone)]
pub struct A2DeskServer {
    tool_router: ToolRouter<Self>,
    input: InputHandle,
}

impl A2DeskServer {
    pub fn new(cfg: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            input: InputHandle::spawn(cfg.prompt_for_permission),
        }
    }
}

// ---------------------------------------------------------------------------
// 参数定义
// ---------------------------------------------------------------------------

/// 宽接受字符串与数字的屏幕选择参数
fn de_screen<'de, D>(d: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.and_then(|v| match v {
        Value::Null => None,
        Value::String(s) => Some(s),
        other => Some(other.to_string()),
    }))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct ScreenshotParams {
    /// 屏幕：索引（如 "0"）、名称关键字（如 "DELL"）或 "primary"；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    screen: Option<String>,
    /// 截图区域（相对该屏幕左上角）；省略=整屏
    #[serde(default)]
    region: Option<Region>,
    /// 输出缩放倍率：1.0（默认）= 1 图片像素对应 1 个屏幕坐标单位，便于把图上坐标直接用于鼠标工具
    #[serde(default)]
    scale: Option<f64>,
    /// 输出图片最大宽度（像素），超出会等比缩小
    #[serde(default)]
    max_width: Option<u32>,
    /// 图片格式：jpeg（默认）或 png
    #[serde(default)]
    format: Option<String>,
    /// JPEG 质量 1-100，默认 85
    #[serde(default)]
    quality: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct MouseMoveParams {
    /// 屏幕：索引、名称关键字或 "primary"；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    screen: Option<String>,
    /// 目标 X（相对该屏幕左上角）
    x: f64,
    /// 目标 Y（相对该屏幕左上角）
    y: f64,
    /// 移动耗时（毫秒），0（默认）= 瞬移，>0 为平滑移动
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct MouseClickParams {
    /// 屏幕：索引、名称关键字或 "primary"；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    screen: Option<String>,
    /// 点击位置 X（相对该屏幕左上角）；省略则使用鼠标当前位置
    #[serde(default)]
    x: Option<f64>,
    /// 点击位置 Y（相对该屏幕左上角）；省略则使用鼠标当前位置
    #[serde(default)]
    y: Option<f64>,
    /// 按键：left（默认）/ middle / right
    #[serde(default)]
    button: Option<String>,
    /// 点击次数 1-3（2=双击），默认 1
    #[serde(default)]
    count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct MouseDoubleClickParams {
    /// 屏幕：索引、名称关键字或 "primary"；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    screen: Option<String>,
    /// 双击位置 X（相对该屏幕左上角）；省略则使用鼠标当前位置
    #[serde(default)]
    x: Option<f64>,
    /// 双击位置 Y（相对该屏幕左上角）；省略则使用鼠标当前位置
    #[serde(default)]
    y: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct MouseDragParams {
    /// 起始屏幕；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    from_screen: Option<String>,
    /// 起始 X
    from_x: f64,
    /// 起始 Y
    from_y: f64,
    /// 目标屏幕；省略=与起始屏幕相同
    #[serde(default, deserialize_with = "de_screen")]
    to_screen: Option<String>,
    /// 目标 X
    to_x: f64,
    /// 目标 Y
    to_y: f64,
    /// 拖拽按键：left（默认）或 right
    #[serde(default)]
    button: Option<String>,
    /// 拖拽耗时（毫秒），默认 500
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct MouseScrollParams {
    /// 屏幕：索引、名称关键字或 "primary"；省略=主屏
    #[serde(default, deserialize_with = "de_screen")]
    screen: Option<String>,
    /// 滚动前先把鼠标移动到的 X（可选）
    #[serde(default)]
    x: Option<f64>,
    /// 滚动前先把鼠标移动到的 Y（可选）
    #[serde(default)]
    y: Option<f64>,
    /// 方向：up / down / left / right
    direction: String,
    /// 滚动格数（1-100，默认 3）
    #[serde(default)]
    amount: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct TypeParams {
    /// 要输入的文本（支持 Unicode；不能包含空字符 \\0）
    text: String,
    /// 每个字符之间的间隔（毫秒），0（默认）= 一次性快速输入
    #[serde(default)]
    interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct PressParams {
    /// 按键列表，每项可以是单键（"Enter"、"a"、"F5"）或组合（"ctrl+shift+s"）；列表内所有按键会一起按下
    keys: Vec<String>,
    /// 重复次数 1-50，默认 1
    #[serde(default)]
    repeat: Option<u32>,
    /// 两次之间间隔（毫秒），默认 100
    #[serde(default)]
    interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct KeyUpDownParams {
    /// 按键列表，每项可以是单键或组合（"ctrl+a"）；传 ["all"] 表示松开全部
    keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct ListAppsParams {
    /// 过滤关键字：匹配进程名 / 可执行文件 / 命令行 / 窗口标题；可用 `|` 分隔多个（任一命中）
    #[serde(default)]
    filter: Option<String>,
    /// 只返回这些 pid
    #[serde(default)]
    pids: Option<Vec<u32>>,
    /// 是否附带窗口信息（标题、位置、所在屏幕），默认 true
    #[serde(default)]
    include_windows: Option<bool>,
    /// 只返回拥有窗口的进程，默认 false
    #[serde(default)]
    only_with_windows: Option<bool>,
    /// 排序：cpu（默认）/ memory / pid / name / start_time
    #[serde(default)]
    sort_by: Option<String>,
    /// 最多返回条数 1-2000，默认 50
    #[serde(default)]
    limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// 工具实现
// ---------------------------------------------------------------------------

#[tool_router]
impl A2DeskServer {
    /// 获取所有屏幕（显示器）的分辨率与位置信息
    #[tool(
        name = "list_screens",
        description = "获取屏幕分辨率：返回所有显示器的索引、名称、尺寸、位置、缩放因子、是否主屏。鼠标/截屏工具的 screen 参数使用这里返回的 index。",
        annotations(title = "获取屏幕分辨率", read_only_hint = true, open_world_hint = false)
    )]
    async fn list_screens(&self) -> ToolResult {
        (|| -> DeskResult<CallToolResult> {
            let screens = screens::all_screens()?;
            let list: Vec<Value> = screens
                .iter()
                .map(|s| serde_json::to_value(&s.info).unwrap_or(Value::Null))
                .collect();
            let primary = screens
                .iter()
                .find(|s| s.info.is_primary)
                .map(|s| s.index)
                .unwrap_or(0);
            let value = json!({
                "count": list.len(),
                "primary_index": primary,
                "screens": list,
                "coordinate_note": "鼠标与截屏 region 使用屏幕内局部坐标（相对该屏幕左上角）；screen 参数可传索引或名称",
            });
            Ok(json_result(value, None))
        })()
        .map_err(DeskError::into_tool_error)
    }

    /// 截取屏幕画面，返回 JPEG（或 PNG）的 base64
    #[tool(
        name = "screenshot",
        description = "截屏：返回图片的 base64（默认 jpeg）。支持指定屏幕与区域(x,y,width,height，相对该屏幕左上角，可选)以及缩放/质量。默认 scale=1.0，即 1 图片像素 = 1 屏幕坐标单位，可直接把图上的像素坐标用作鼠标工具坐标。",
        annotations(title = "截屏", read_only_hint = true, open_world_hint = false)
    )]
    async fn screenshot(&self, Parameters(p): Parameters<ScreenshotParams>) -> ToolResult {
        self.screenshot_impl(p)
            .await
            .map_err(DeskError::into_tool_error)
    }

    /// 把鼠标移动到指定屏幕的指定位置
    #[tool(
        name = "mouse_move",
        description = "鼠标移动：把鼠标移动到指定屏幕的 (x, y)，坐标为屏幕内局部坐标。可选 duration_ms 做平滑移动。",
        annotations(title = "鼠标移动", read_only_hint = false, open_world_hint = false)
    )]
    async fn mouse_move(&self, Parameters(p): Parameters<MouseMoveParams>) -> ToolResult {
        self.mouse_move_impl(p).await.map_err(DeskError::into_tool_error)
    }

    /// 鼠标点击（左/中/右键，支持单击/双击/三击）
    #[tool(
        name = "mouse_click",
        description = "鼠标点击：在指定屏幕的 (x, y) 点击，button 可选 left/middle/right，count 为次数（1=单击，2=双击）。省略 x/y 则在鼠标当前位置点击。",
        annotations(title = "鼠标点击", read_only_hint = false, open_world_hint = false)
    )]
    async fn mouse_click(&self, Parameters(p): Parameters<MouseClickParams>) -> ToolResult {
        self.mouse_click_impl(p).await.map_err(DeskError::into_tool_error)
    }

    /// 鼠标双击（固定左键）
    #[tool(
        name = "mouse_double_click",
        description = "鼠标双击：在指定屏幕的 (x, y) 用左键双击。省略 x/y 则在鼠标当前位置双击。",
        annotations(title = "鼠标双击", read_only_hint = false, open_world_hint = false)
    )]
    async fn mouse_double_click(
        &self,
        Parameters(p): Parameters<MouseDoubleClickParams>,
    ) -> ToolResult {
        self.mouse_double_click_impl(p)
            .await
            .map_err(DeskError::into_tool_error)
    }

    /// 鼠标拖拽（按下 -> 移动 -> 松开）
    #[tool(
        name = "mouse_drag",
        description = "鼠标拖拽：在起始屏幕的 (from_x, from_y) 按下按键，移动到目标屏幕的 (to_x, to_y) 后松开。支持跨屏，button 可选 left（默认）/right。",
        annotations(title = "鼠标拖拽", read_only_hint = false, open_world_hint = false)
    )]
    async fn mouse_drag(&self, Parameters(p): Parameters<MouseDragParams>) -> ToolResult {
        self.mouse_drag_impl(p).await.map_err(DeskError::into_tool_error)
    }

    /// 鼠标滚轮滚动
    #[tool(
        name = "mouse_scroll",
        description = "鼠标滚动：在指定屏幕（可选先移动到 x,y）向 up/down/left/right 滚动 amount 格（1-100，默认 3）。",
        annotations(title = "鼠标滚动", read_only_hint = false, open_world_hint = false)
    )]
    async fn mouse_scroll(&self, Parameters(p): Parameters<MouseScrollParams>) -> ToolResult {
        self.mouse_scroll_impl(p)
            .await
            .map_err(DeskError::into_tool_error)
    }

    /// 获取当前鼠标位置
    #[tool(
        name = "mouse_position",
        description = "获取鼠标当前位置（虚拟桌面绝对坐标）以及它所在的屏幕。",
        annotations(title = "获取鼠标位置", read_only_hint = true, open_world_hint = false)
    )]
    async fn mouse_position(&self) -> ToolResult {
        self.mouse_position_impl()
            .await
            .map_err(DeskError::into_tool_error)
    }

    /// 输入文本
    #[tool(
        name = "keyboard_type",
        description = "键盘输入文本：支持 Unicode，可选 interval_ms 控制逐字输入速度（0=最快）。",
        annotations(title = "输入文本", read_only_hint = false, open_world_hint = false)
    )]
    async fn keyboard_type(&self, Parameters(p): Parameters<TypeParams>) -> ToolResult {
        self.keyboard_type_impl(p).await.map_err(DeskError::into_tool_error)
    }

    /// 按键 / 组合键（按下并松开）
    #[tool(
        name = "keyboard_press",
        description = "按键：按下一组按键并松开。keys 每项可以是单键（\"Enter\"、\"a\"、\"F5\"、\"Delete\"）或组合（\"ctrl+shift+s\"）；列表内所有按键视为一个组合一起按下。支持 repeat 重复。",
        annotations(title = "按键", read_only_hint = false, open_world_hint = false)
    )]
    async fn keyboard_press(&self, Parameters(p): Parameters<PressParams>) -> ToolResult {
        self.keyboard_press_impl(p).await.map_err(DeskError::into_tool_error)
    }

    /// 按下按键（不松开）
    #[tool(
        name = "keyboard_key_down",
        description = "按下按键并保持（不松开），用于组合键或长按。可用 keyboard_key_up 或 keyboard_key_up([\"all\"]) 松开。",
        annotations(title = "按下按键", read_only_hint = false, open_world_hint = false)
    )]
    async fn keyboard_key_down(&self, Parameters(p): Parameters<KeyUpDownParams>) -> ToolResult {
        self.key_state_impl(p.keys, true).await.map_err(DeskError::into_tool_error)
    }

    /// 松开按键
    #[tool(
        name = "keyboard_key_up",
        description = "松开按键。keys 传 [\"all\"] 时松开本服务按下过的所有按键（用于从意外卡键中恢复）。",
        annotations(title = "松开按键", read_only_hint = false, open_world_hint = false)
    )]
    async fn keyboard_key_up(&self, Parameters(p): Parameters<KeyUpDownParams>) -> ToolResult {
        self.key_state_impl(p.keys, false).await.map_err(DeskError::into_tool_error)
    }

    /// 获取正在运行的应用（进程）与窗口信息
    #[tool(
        name = "list_apps",
        description = "获取正在运行的程序信息：返回 pid、进程名、可执行文件、命令行、CPU/内存占用以及所属窗口（标题、位置、所在屏幕）。支持按关键字（名称/路径/标题）过滤、按 pid 过滤、排序与限量。",
        annotations(title = "获取运行中的应用", read_only_hint = true, open_world_hint = false)
    )]
    async fn list_apps(&self, Parameters(p): Parameters<ListAppsParams>) -> ToolResult {
        self.list_apps_impl(p).await.map_err(DeskError::into_tool_error)
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "a2desk",
    instructions = "a2desk 桌面控制：截屏、鼠标、键盘、应用枚举。所有鼠标工具与截屏 region 统一使用「屏幕内局部坐标」（相对该屏幕左上角），用 screen 参数（索引/名称/primary，省略=主屏）指定屏幕。推荐流程：list_screens -> screenshot(scale=1.0 时 1 图片像素=1 坐标单位) -> mouse_click。"
)]
impl rmcp::ServerHandler for A2DeskServer {}

// ---------------------------------------------------------------------------
// 具体实现
// ---------------------------------------------------------------------------

/// 一个已解析、已裁剪的坐标点。
///
/// 这里**只保存纯数据**：`xcap::Monitor` 在 Windows 上含有裸指针（不是 `Send`），
/// 一旦被 async 工具函数跨 `await` 持有，整个 future 就不再是 `Send`。
struct Target {
    screen_index: usize,
    screen_name: String,
    screen_width: u32,
    screen_height: u32,
    local_x: i32,
    local_y: i32,
    global_x: i32,
    global_y: i32,
    clamped: bool,
}

impl Target {
    fn describe(&self, label: &str) -> Value {
        let mut v = json!({
            "screen_index": self.screen_index,
            "screen_name": self.screen_name,
            "x": self.local_x,
            "y": self.local_y,
            "global_x": self.global_x,
            "global_y": self.global_y,
        });
        if self.clamped {
            v["clamped"] = json!(true);
            v["clamp_note"] = json!(format!(
                "{label} 超出屏幕范围，已裁剪到 ({}, {})，该屏有效范围 0..={} / 0..={}",
                self.local_x,
                self.local_y,
                self.screen_width.saturating_sub(1),
                self.screen_height.saturating_sub(1)
            ));
        }
        v
    }
}

fn to_i32(v: f64, name: &str) -> DeskResult<i32> {
    if !v.is_finite() {
        return Err(DeskError::InvalidArgument(format!("{name} 不是有效数字")));
    }
    let r = v.round();
    if r < i32::MIN as f64 || r > i32::MAX as f64 {
        return Err(DeskError::InvalidArgument(format!("{name} 超出范围")));
    }
    Ok(r as i32)
}

fn resolve_target(screen: Option<&str>, x: f64, y: f64) -> DeskResult<Target> {
    let resolved = screens::resolve_screen(screen)?;
    let info = &resolved.info;
    let (max_x, max_y) = info.local_max();
    let lx = to_i32(x, "x")? as i64;
    let ly = to_i32(y, "y")? as i64;
    let cx = lx.clamp(0, max_x.max(0)) as i32;
    let cy = ly.clamp(0, max_y.max(0)) as i32;
    Ok(Target {
        screen_index: resolved.index,
        screen_name: info.name.clone(),
        screen_width: info.width,
        screen_height: info.height,
        global_x: info.x + cx,
        global_y: info.y + cy,
        local_x: cx,
        local_y: cy,
        clamped: (cx as i64) != lx || (cy as i64) != ly,
    })
}

fn json_result(value: Value, note: Option<String>) -> CallToolResult {
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    if let Some(note) = note {
        text = format!("{note}\n{text}");
    }
    let mut r = CallToolResult::structured(value);
    r.content = vec![ContentBlock::text(text)];
    r
}

fn ok_result(value: Value) -> DeskResult<CallToolResult> {
    Ok(json_result(value, None))
}

impl A2DeskServer {
    async fn screenshot_impl(&self, p: ScreenshotParams) -> DeskResult<CallToolResult> {
        let screen = screens::resolve_screen(p.screen.as_deref())?;
        let format = OutFormat::parse(p.format.as_deref().unwrap_or("jpeg"))?;
        let quality = p.quality.unwrap_or(85).clamp(1, 100);
        let scale = p.scale.unwrap_or(1.0);
        if !scale.is_finite() || scale <= 0.0 || scale > 8.0 {
            return Err(DeskError::InvalidArgument(format!(
                "scale 必须在 (0, 8] 之间，当前为 {scale}"
            )));
        }

        let opts = CaptureOptions {
            region: p.region,
            scale,
            max_width: p.max_width.filter(|w| *w > 0),
            format,
            quality,
        };

        let (bytes, meta) = screens::capture(&screen, &opts)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let mut value = serde_json::to_value(&meta).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut value {
            map.insert("screen_index".into(), json!(screen.index));
            map.insert("screen_name".into(), json!(screen.info.name));
            map.insert("screen_width".into(), json!(screen.info.width));
            map.insert("screen_height".into(), json!(screen.info.height));
            map.insert("scale_factor".into(), json!(screen.info.scale_factor));
            map.insert(
                "coordinate_hint".into(),
                json!(format!(
                    "屏幕坐标X = region.x + 图片X / pixel_ratio；屏幕坐标Y = region.y + 图片Y / pixel_ratio（当前 pixel_ratio = {:.4}）",
                    meta.pixel_ratio
                )),
            );
        }

        let mut note = Vec::new();
        if meta.region_clamped {
            note.push(format!(
                "注意：请求的区域超出屏幕，已裁剪为 ({},{},{}x{})",
                meta.region.x, meta.region.y, meta.region.width, meta.region.height
            ));
        }
        let note = if note.is_empty() {
            None
        } else {
            Some(note.join("\n"))
        };

        let text = {
            let mut t = serde_json::to_string_pretty(&value).unwrap_or_default();
            if let Some(n) = &note {
                t = format!("{n}\n{t}");
            }
            t
        };

        let mut result = CallToolResult::success(vec![
            ContentBlock::image(b64, format.mime()),
            ContentBlock::text(text),
        ]);
        result.structured_content = Some(value);
        Ok(result)
    }

    async fn mouse_move_impl(&self, p: MouseMoveParams) -> DeskResult<CallToolResult> {
        let t = resolve_target(p.screen.as_deref(), p.x, p.y)?;
        let duration = p.duration_ms.unwrap_or(0).min(10_000);
        self.input
            .move_mouse(t.global_x, t.global_y, duration)
            .await?;
        let mut v = t.describe("目标点");
        v["action"] = json!("mouse_move");
        v["duration_ms"] = json!(duration);
        ok_result(v)
    }

    async fn mouse_click_impl(&self, p: MouseClickParams) -> DeskResult<CallToolResult> {
        let button = MouseButton::parse(p.button.as_deref().unwrap_or("left"))?;
        let count = p.count.unwrap_or(1).clamp(1, 3);
        let target = match (p.x, p.y) {
            (Some(x), Some(y)) => Some(resolve_target(p.screen.as_deref(), x, y)?),
            (None, None) => None,
            _ => {
                return Err(DeskError::InvalidArgument(
                    "x 与 y 必须同时提供或同时省略".into(),
                ))
            }
        };
        let at = target.as_ref().map(|t| (t.global_x, t.global_y));
        self.input.click_at(button, count, at).await?;
        let mut v = match &target {
            Some(t) => t.describe("点击位置"),
            None => current_position_value(&self.input).await?,
        };
        v["action"] = json!("mouse_click");
        v["button"] = json!(button.as_str());
        v["count"] = json!(count);
        ok_result(v)
    }

    async fn mouse_double_click_impl(&self, p: MouseDoubleClickParams) -> DeskResult<CallToolResult> {
        let target = match (p.x, p.y) {
            (Some(x), Some(y)) => Some(resolve_target(p.screen.as_deref(), x, y)?),
            (None, None) => None,
            _ => {
                return Err(DeskError::InvalidArgument(
                    "x 与 y 必须同时提供或同时省略".into(),
                ))
            }
        };
        let at = target.as_ref().map(|t| (t.global_x, t.global_y));
        self.input.click_at(MouseButton::Left, 2, at).await?;
        let mut v = match &target {
            Some(t) => t.describe("双击位置"),
            None => current_position_value(&self.input).await?,
        };
        v["action"] = json!("mouse_double_click");
        v["button"] = json!("left");
        v["count"] = json!(2);
        ok_result(v)
    }

    async fn mouse_drag_impl(&self, p: MouseDragParams) -> DeskResult<CallToolResult> {
        let button = MouseButton::parse(p.button.as_deref().unwrap_or("left"))?;
        if button == MouseButton::Middle {
            return Err(DeskError::InvalidArgument(
                "拖拽只支持 left 或 right 按键".into(),
            ));
        }
        let from = resolve_target(p.from_screen.as_deref(), p.from_x, p.from_y)?;
        let to_sel = p.to_screen.as_deref().or(p.from_screen.as_deref());
        let to = resolve_target(to_sel, p.to_x, p.to_y)?;
        let duration = p.duration_ms.unwrap_or(500).clamp(0, 10_000);

        self.input
            .drag(
                (from.global_x, from.global_y),
                (to.global_x, to.global_y),
                button,
                duration,
            )
            .await?;

        let mut v = json!({
            "action": "mouse_drag",
            "button": button.as_str(),
            "duration_ms": duration,
            "from": from.describe("起点"),
            "to": to.describe("终点"),
        });
        if from.clamped || to.clamped {
            v["clamped"] = json!(true);
        }
        ok_result(v)
    }

    async fn mouse_scroll_impl(&self, p: MouseScrollParams) -> DeskResult<CallToolResult> {
        let dir = ScrollDir::parse(&p.direction)?;
        let amount = p.amount.unwrap_or(3);
        if amount == 0 || amount > MAX_SCROLL_AMOUNT {
            return Err(DeskError::InvalidArgument(format!(
                "amount 必须在 1..={MAX_SCROLL_AMOUNT} 之间（当前 {amount}）"
            )));
        }

        let target = match (p.x, p.y) {
            (Some(x), Some(y)) => Some(resolve_target(p.screen.as_deref(), x, y)?),
            (None, None) => None,
            _ => {
                return Err(DeskError::InvalidArgument(
                    "x 与 y 必须同时提供或同时省略".into(),
                ))
            }
        };
        let at = target.as_ref().map(|t| (t.global_x, t.global_y));

        self.input.scroll(dir, amount, at).await?;

        let mut v = match &target {
            Some(t) => t.describe("滚动位置"),
            None => current_position_value(&self.input).await?,
        };
        v["action"] = json!("mouse_scroll");
        v["direction"] = json!(dir.as_str());
        v["amount"] = json!(amount);
        ok_result(v)
    }

    async fn mouse_position_impl(&self) -> DeskResult<CallToolResult> {
        let (x, y) = self.input.position().await?;
        let screens = screens::all_screens()?;
        let on = screens
            .iter()
            .find(|s| {
                let w = s.info.width as i32;
                let h = s.info.height as i32;
                x >= s.info.x && x < s.info.x + w && y >= s.info.y && y < s.info.y + h
            })
            .or_else(|| screens.first());
        let mut v = json!({ "global_x": x, "global_y": y });
        if let Some(s) = on {
            v["screen_index"] = json!(s.index);
            v["screen_name"] = json!(s.info.name);
            v["local_x"] = json!(x - s.info.x);
            v["local_y"] = json!(y - s.info.y);
        }
        ok_result(v)
    }

    async fn keyboard_type_impl(&self, p: TypeParams) -> DeskResult<CallToolResult> {
        if p.text.contains('\0') {
            return Err(DeskError::InvalidArgument(
                "text 不能包含空字符 \\0".into(),
            ));
        }
        let interval = p.interval_ms.unwrap_or(0).min(5_000);
        let len = p.text.chars().count();
        self.input.text(p.text, interval).await?;
        ok_result(json!({
            "action": "keyboard_type",
            "chars": len,
            "interval_ms": interval,
        }))
    }

    async fn keyboard_press_impl(&self, p: PressParams) -> DeskResult<CallToolResult> {
        let keys = parse_key_group(&p.keys)?;
        let repeat = p.repeat.unwrap_or(1).clamp(1, 50);
        let interval = p.interval_ms.unwrap_or(100).min(10_000);
        let names = describe_keys(&keys);
        self.input.chord(keys, repeat, interval).await?;
        ok_result(json!({
            "action": "keyboard_press",
            "keys": names,
            "repeat": repeat,
            "interval_ms": interval,
        }))
    }

    async fn key_state_impl(&self, keys: Vec<String>, down: bool) -> DeskResult<CallToolResult> {
        if !down {
            let release_all = keys
                .iter()
                .any(|k| k.trim().eq_ignore_ascii_case("all") || k.trim() == "全部");
            if release_all {
                let held = self.input.release_all().await?;
                return ok_result(json!({
                    "action": "keyboard_key_up_all",
                    "still_pressed": held,
                }));
            }
        }

        let parsed = parse_key_group(&keys)?;
        let names = describe_keys(&parsed);
        let still = self.input.key_state(parsed, down).await?;
        ok_result(json!({
            "action": if down { "keyboard_key_down" } else { "keyboard_key_up" },
            "keys": names,
            "still_pressed": still,
        }))
    }

    async fn list_apps_impl(&self, p: ListAppsParams) -> DeskResult<CallToolResult> {
        let query = AppQuery {
            filter: p.filter,
            pids: p.pids,
            include_windows: p.include_windows.unwrap_or(true),
            only_with_windows: p.only_with_windows.unwrap_or(false),
            sort_by: AppSort::parse(p.sort_by.as_deref().unwrap_or("cpu"))?,
            limit: p.limit.unwrap_or(50),
        };
        let list = apps::list_apps(&query)?;
        let items: Vec<Value> = list
            .apps
            .iter()
            .map(|a| serde_json::to_value(a).unwrap_or(Value::Null))
            .collect();
        let mut value = json!({
            "total_matched": list.total_matched,
            "returned": items.len(),
            "truncated": list.truncated,
            "windows_available": list.windows_available,
            "apps": items,
        });
        if !list.warnings.is_empty() {
            value["warnings"] = json!(list.warnings);
        }
        ok_result(value)
    }
}

fn describe_keys(keys: &[enigo::Key]) -> Vec<String> {
    keys.iter()
        .map(|k| match k {
            enigo::Key::Unicode(c) => c.to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

async fn current_position_value(input: &InputHandle) -> DeskResult<Value> {
    let (x, y) = input.position().await?;
    let screens = screens::all_screens()?;
    let mut v = json!({ "global_x": x, "global_y": y });
    if let Some(s) = screens.iter().find(|s| {
        let w = s.info.width as i32;
        let h = s.info.height as i32;
        x >= s.info.x && x < s.info.x + w && y >= s.info.y && y < s.info.y + h
    }) {
        v["screen_index"] = json!(s.index);
        v["screen_name"] = json!(s.info.name);
        v["x"] = json!(x - s.info.x);
        v["y"] = json!(y - s.info.y);
    }
    Ok(v)
}