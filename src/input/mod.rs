//! 鼠标 / 键盘输入控制。
//!
//! `enigo` 的实例不是 `Send`，而且输入事件本身需要串行执行（按下-移动-松开），
//! 因此这里用一个专属线程持有 `Enigo`，异步侧只通过 channel 下发命令。

pub mod keys;

use std::thread;
use std::time::{Duration, Instant};

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use tokio::sync::{mpsc, oneshot};

use crate::error::{DeskError, DeskResult};

pub use keys::{parse_key, parse_key_group};

/// 鼠标按键
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    pub fn parse(s: &str) -> DeskResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "left" | "l" | "左键" | "左" => Ok(MouseButton::Left),
            "middle" | "m" | "center" | "中键" | "中" => Ok(MouseButton::Middle),
            "right" | "r" | "右键" | "右" => Ok(MouseButton::Right),
            other => Err(DeskError::InvalidArgument(format!(
                "不支持的鼠标按键 `{other}`，可选：left、middle、right"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Middle => "middle",
            MouseButton::Right => "right",
        }
    }

    fn to_enigo(self) -> Button {
        match self {
            MouseButton::Left => Button::Left,
            MouseButton::Middle => Button::Middle,
            MouseButton::Right => Button::Right,
        }
    }
}

/// 滚动方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDir {
    pub fn parse(s: &str) -> DeskResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "up" | "u" | "上" | "向上" => Ok(ScrollDir::Up),
            "down" | "d" | "下" | "向下" => Ok(ScrollDir::Down),
            "left" | "l" | "左" | "向左" => Ok(ScrollDir::Left),
            "right" | "r" | "右" | "向右" => Ok(ScrollDir::Right),
            other => Err(DeskError::InvalidArgument(format!(
                "不支持的滚动方向 `{other}`，可选：up、down、left、right"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ScrollDir::Up => "up",
            ScrollDir::Down => "down",
            ScrollDir::Left => "left",
            ScrollDir::Right => "right",
        }
    }
}

type Reply<T> = oneshot::Sender<DeskResult<T>>;

enum Request {
    Move {
        x: i32,
        y: i32,
        duration_ms: u64,
        reply: Reply<()>,
    },
    Click {
        button: MouseButton,
        count: u32,
        /// 需要先移动到该绝对坐标（并等待系统位置视图跟上）再点击
        at: Option<(i32, i32)>,
        reply: Reply<()>,
    },
    Drag {
        from: (i32, i32),
        to: (i32, i32),
        button: MouseButton,
        duration_ms: u64,
        reply: Reply<()>,
    },
    Scroll {
        dir: ScrollDir,
        amount: u32,
        /// 需要先把光标放到该绝对坐标再滚动
        at: Option<(i32, i32)>,
        reply: Reply<()>,
    },
    Text {
        text: String,
        interval_ms: u64,
        reply: Reply<()>,
    },
    Chord {
        keys: Vec<Key>,
        repeat: u32,
        interval_ms: u64,
        reply: Reply<()>,
    },
    KeyState {
        keys: Vec<Key>,
        down: bool,
        reply: Reply<Vec<String>>,
    },
    ReleaseAll {
        reply: Reply<Vec<String>>,
    },
    Position {
        reply: Reply<(i32, i32)>,
    },
}

/// 输入控制器句柄（可克隆，内部是 channel）
#[derive(Debug, Clone)]
pub struct InputHandle {
    tx: mpsc::UnboundedSender<Request>,
}

impl InputHandle {
    /// 启动输入工作线程
    pub fn spawn(prompt_for_permission: bool) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Request>();

        let _ = thread::Builder::new()
            .name("a2desk-input".into())
            .spawn(move || {
                let settings = Settings {
                    open_prompt_to_get_permissions: prompt_for_permission,
                    // 输入的按键/点击与物理键盘状态解耦，避免受用户正在按住的修饰键影响
                    independent_of_keyboard_state: true,
                    ..Settings::default()
                };

                let mut worker = Worker {
                    settings,
                    enigo: None,
                    pressed: Vec::new(),
                };

                while let Some(req) = rx.blocking_recv() {
                    worker.handle(req);
                }
                tracing::debug!("input worker 退出");
            });

        Self { tx }
    }

    async fn request<T>(
        &self,
        build: impl FnOnce(Reply<T>) -> Request,
    ) -> DeskResult<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(build(tx))
            .map_err(|_| DeskError::InputWorkerGone)?;
        rx.await.map_err(|_| DeskError::InputWorkerGone)?
    }

    /// 把鼠标移动到全屏绝对坐标
    pub async fn move_mouse(&self, x: i32, y: i32, duration_ms: u64) -> DeskResult<()> {
        self.request(|reply| Request::Move {
            x,
            y,
            duration_ms,
            reply,
        })
        .await
    }

    /// 点击（count=2 即双击）。
    ///
    /// `at` 给出时，会先移动到该绝对坐标并等系统位置视图同步后再点击——
    /// 这一步不能省：enigo 在 macOS 上构造点击事件时会自己再读一次当前坐标，
    /// 移动后立刻点击会被投递到旧坐标（详见 `move_and_settle`）。
    pub async fn click_at(
        &self,
        button: MouseButton,
        count: u32,
        at: Option<(i32, i32)>,
    ) -> DeskResult<()> {
        self.request(|reply| Request::Click {
            button,
            count,
            at,
            reply,
        })
        .await
    }

    /// 拖拽：在起点按下 -> 移动到终点 -> 松开
    pub async fn drag(
        &self,
        from: (i32, i32),
        to: (i32, i32),
        button: MouseButton,
        duration_ms: u64,
    ) -> DeskResult<()> {
        self.request(|reply| Request::Drag {
            from,
            to,
            button,
            duration_ms,
            reply,
        })
        .await
    }

    /// 滚动，amount 为滚轮格数（1..=100）
    pub async fn scroll(
        &self,
        dir: ScrollDir,
        amount: u32,
        at: Option<(i32, i32)>,
    ) -> DeskResult<()> {
        self.request(|reply| Request::Scroll {
            dir,
            amount,
            at,
            reply,
        })
        .await
    }

    /// 输入文本
    pub async fn text(&self, text: String, interval_ms: u64) -> DeskResult<()> {
        self.request(|reply| Request::Text {
            text,
            interval_ms,
            reply,
        })
        .await
    }

    /// 组合键点击
    pub async fn chord(&self, keys: Vec<Key>, repeat: u32, interval_ms: u64) -> DeskResult<()> {
        self.request(|reply| Request::Chord {
            keys,
            repeat,
            interval_ms,
            reply,
        })
        .await
    }

    /// 按下/松开一组按键，返回当前仍处于按下状态的按键名
    pub async fn key_state(&self, keys: Vec<Key>, down: bool) -> DeskResult<Vec<String>> {
        self.request(|reply| Request::KeyState { keys, down, reply })
            .await
    }

    /// 松开所有由本服务按下的按键
    pub async fn release_all(&self) -> DeskResult<Vec<String>> {
        self.request(|reply| Request::ReleaseAll { reply }).await
    }

    /// 当前鼠标位置（虚拟桌面绝对坐标）
    pub async fn position(&self) -> DeskResult<(i32, i32)> {
        self.request(|reply| Request::Position { reply }).await
    }
}

struct Worker {
    settings: Settings,
    enigo: Option<Enigo>,
    pressed: Vec<Key>,
}

impl Worker {
    fn enigo(&mut self) -> DeskResult<&mut Enigo> {
        if self.enigo.is_none() {
            let enigo = Enigo::new(&self.settings).map_err(|e| {
                DeskError::Input(format!("初始化输入控制失败：{e:?}")).with_hint(permission_hint())
            })?;
            self.enigo = Some(enigo);
        }
        Ok(self.enigo.as_mut().expect("just set"))
    }

    fn handle(&mut self, req: Request) {
        match req {
            Request::Move {
                x,
                y,
                duration_ms,
                reply,
            } => {
                let r = self.do_move(x, y, duration_ms);
                let _ = reply.send(r);
            }
            Request::Click {
                button,
                count,
                at,
                reply,
            } => {
                let r = match at {
                    Some((x, y)) => self
                        .move_and_settle(x, y)
                        .and_then(|_| self.do_click(button, count)),
                    None => self.do_click(button, count),
                };
                let _ = reply.send(r);
            }
            Request::Drag {
                from,
                to,
                button,
                duration_ms,
                reply,
            } => {
                let r = self.do_drag(from, to, button, duration_ms);
                let _ = reply.send(r);
            }
            Request::Scroll {
                dir,
                amount,
                at,
                reply,
            } => {
                let r = match at {
                    Some((x, y)) => self
                        .move_and_settle(x, y)
                        .and_then(|_| self.do_scroll(dir, amount)),
                    None => self.do_scroll(dir, amount),
                };
                let _ = reply.send(r);
            }
            Request::Text {
                text,
                interval_ms,
                reply,
            } => {
                let r = self.do_text(&text, interval_ms);
                let _ = reply.send(r);
            }
            Request::Chord {
                keys,
                repeat,
                interval_ms,
                reply,
            } => {
                let r = self.do_chord(&keys, repeat, interval_ms);
                let _ = reply.send(r);
            }
            Request::KeyState { keys, down, reply } => {
                let r = self.do_key_state(&keys, down);
                let _ = reply.send(r);
            }
            Request::ReleaseAll { reply } => {
                let r = self.do_release_all();
                let _ = reply.send(r);
            }
            Request::Position { reply } => {
                let r = self
                    .enigo()
                    .and_then(|e| e.location().map_err(input_err));
                let _ = reply.send(r);
            }
        }
    }

    fn do_move(&mut self, x: i32, y: i32, duration_ms: u64) -> DeskResult<()> {
        let enigo = self.enigo()?;
        let from = enigo.location().unwrap_or((x, y));
        smooth_move(enigo, from, (x, y), duration_ms)
    }

    /// 把光标移动到 (x, y)，然后等系统（以及 enigo `location()` 的视图）真正跟上。
    ///
    /// 为什么必须等：enigo 在 macOS 上实现 `button()` / `key()` 时会**自己再调用一次
    /// `location()`** 来决定事件投递坐标。如果移动之后立刻点击，`location()` 很可能仍是
    /// 旧值，于是点击落在旧坐标上，并把光标"拉回"旧位置——表现为"`mouse_click` 带坐标
    /// 完全没生效"。这里轮询到位置一致再继续，代价是几毫秒。
    fn move_and_settle(&mut self, x: i32, y: i32) -> DeskResult<()> {
        {
            let enigo = self.enigo()?;
            abs_move(enigo, x, y)?;
        }
        self.settle(x, y);
        Ok(())
    }

    /// 轮询等待 enigo 看到的鼠标位置变成 (x, y)（最多 `SETTLE_TIMEOUT_MS` 毫秒）
    fn settle(&mut self, x: i32, y: i32) {
        let deadline = Instant::now() + Duration::from_millis(SETTLE_TIMEOUT_MS);
        loop {
            let matched = match self.enigo() {
                Ok(enigo) => matches!(enigo.location(), Ok(p) if p == (x, y)),
                Err(_) => return,
            };
            if matched || Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(3));
        }
    }

    fn do_click(&mut self, button: MouseButton, count: u32) -> DeskResult<()> {
        let count = count.clamp(1, 3);
        let enigo = self.enigo()?;
        let btn = button.to_enigo();
        for i in 0..count {
            enigo.button(btn, Direction::Click).map_err(input_err)?;
            if i + 1 < count {
                thread::sleep(Duration::from_millis(60));
            }
        }
        Ok(())
    }

    fn do_drag(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        button: MouseButton,
        duration_ms: u64,
    ) -> DeskResult<()> {
        let btn = button.to_enigo();
        // 先定位并等待位置同步，否则按下事件会被投递到旧坐标
        self.move_and_settle(from.0, from.1)?;
        thread::sleep(Duration::from_millis(30));
        {
            let enigo = self.enigo()?;
            enigo.button(btn, Direction::Press).map_err(input_err)?;
        }
        thread::sleep(Duration::from_millis(50));
        let r = {
            let enigo = self.enigo()?;
            smooth_move(enigo, from, to, duration_ms)
        };
        // 松开前也确认位置已到终点
        self.settle(to.0, to.1);
        thread::sleep(Duration::from_millis(50));
        // 无论如何都要松开按键，避免鼠标卡在按下状态
        let release = self
            .enigo()
            .and_then(|enigo| enigo.button(btn, Direction::Release).map_err(input_err));
        r?;
        release
    }

    fn do_scroll(&mut self, dir: ScrollDir, amount: u32) -> DeskResult<()> {
        let amount = amount.clamp(1, MAX_SCROLL_AMOUNT);
        let (axis, sign) = match dir {
            ScrollDir::Up => (Axis::Vertical, -1),
            ScrollDir::Down => (Axis::Vertical, 1),
            ScrollDir::Left => (Axis::Horizontal, -1),
            ScrollDir::Right => (Axis::Horizontal, 1),
        };
        let enigo = self.enigo()?;
        let mut left = amount as i32;
        while left > 0 {
            let step = left.min(SCROLL_CHUNK) * sign;
            enigo.scroll(step, axis).map_err(input_err)?;
            left -= left.min(SCROLL_CHUNK);
            if left > 0 {
                thread::sleep(Duration::from_millis(12));
            }
        }
        Ok(())
    }

    fn do_text(&mut self, text: &str, interval_ms: u64) -> DeskResult<()> {
        if text.is_empty() {
            return Ok(());
        }
        let enigo = self.enigo()?;
        if interval_ms == 0 {
            return enigo.text(text).map_err(input_err);
        }
        for c in text.chars() {
            enigo.key(Key::Unicode(c), Direction::Click).map_err(input_err)?;
            thread::sleep(Duration::from_millis(interval_ms));
        }
        Ok(())
    }

    fn do_chord(&mut self, keys: &[Key], repeat: u32, interval_ms: u64) -> DeskResult<()> {
        let repeat = repeat.clamp(1, 50);
        let enigo = self.enigo()?;
        for round in 0..repeat {
            for k in keys {
                enigo.key(*k, Direction::Press).map_err(input_err)?;
                thread::sleep(Duration::from_millis(15));
            }
            thread::sleep(Duration::from_millis(35));
            for k in keys.iter().rev() {
                enigo.key(*k, Direction::Release).map_err(input_err)?;
                thread::sleep(Duration::from_millis(12));
            }
            if round + 1 < repeat {
                thread::sleep(Duration::from_millis(interval_ms.max(50)));
            }
        }
        Ok(())
    }

    fn do_key_state(&mut self, keys: &[Key], down: bool) -> DeskResult<Vec<String>> {
        {
            let enigo = self.enigo()?;
            if down {
                for k in keys {
                    enigo.key(*k, Direction::Press).map_err(input_err)?;
                    thread::sleep(Duration::from_millis(15));
                }
            } else {
                for k in keys.iter().rev() {
                    enigo.key(*k, Direction::Release).map_err(input_err)?;
                    thread::sleep(Duration::from_millis(12));
                }
            }
        }

        if down {
            for k in keys {
                if !self.pressed.contains(k) {
                    self.pressed.push(*k);
                }
            }
        } else {
            self.pressed.retain(|p| !keys.contains(p));
        }

        Ok(self.pressed.iter().map(key_name).collect())
    }

    fn do_release_all(&mut self) -> DeskResult<Vec<String>> {
        let keys = std::mem::take(&mut self.pressed);
        let enigo = self.enigo()?;
        for k in keys.iter().rev() {
            let _ = enigo.key(*k, Direction::Release);
            thread::sleep(Duration::from_millis(10));
        }
        Ok(Vec::new())
    }
}

/// 滚动格数上限（范围控制）
pub const MAX_SCROLL_AMOUNT: u32 = 100;
const SCROLL_CHUNK: i32 = 3;
/// 等待系统同步鼠标位置的最长时间（毫秒）
const SETTLE_TIMEOUT_MS: u64 = 400;
const DOUBLE_CLICK_GAP_MS: u64 = 60;

fn smooth_move(enigo: &mut Enigo, from: (i32, i32), to: (i32, i32), duration_ms: u64) -> DeskResult<()> {
    if duration_ms == 0 || from == to {
        return abs_move(enigo, to.0, to.1);
    }
    let steps = (duration_ms / 10).clamp(2, 120) as u32;
    let step_delay = Duration::from_millis((duration_ms / steps as u64).max(1));
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = from.0 as f64 + (to.0 - from.0) as f64 * t;
        let y = from.1 as f64 + (to.1 - from.1) as f64 * t;
        abs_move(enigo, x.round() as i32, y.round() as i32)?;
        if i < steps {
            thread::sleep(step_delay);
        }
    }
    abs_move(enigo, to.0, to.1)
}

#[cfg(not(target_os = "windows"))]
fn abs_move(enigo: &mut Enigo, x: i32, y: i32) -> DeskResult<()> {
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(input_err)
}

/// Windows 上 enigo 的绝对移动只覆盖主显示器（内部把坐标按主屏尺寸归一化），
/// 这里改用 `SetCursorPos`，它使用整个虚拟桌面的物理像素坐标，支持多屏。
#[cfg(target_os = "windows")]
fn abs_move(enigo: &mut Enigo, x: i32, y: i32) -> DeskResult<()> {
    let ok = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y) };
    if ok != 0 {
        return Ok(());
    }
    enigo.move_mouse(x, y, Coordinate::Abs).map_err(input_err)
}

fn input_err(e: enigo::InputError) -> DeskError {
    DeskError::Input(format!("{e}")).with_hint(permission_hint())
}

fn permission_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS 需要在「系统设置 → 隐私与安全性 → 辅助功能」里，为运行 a2desk 的程序\
         （终端 App 或 MCP 客户端本体）勾选授权。\
         注意：「辅助功能」和截屏所需的「屏幕录制」是两个彼此独立的权限，只勾「屏幕录制」无法模拟鼠标键盘。\
         并且 TCC 会在进程内缓存判定结果，授权后必须完全退出并重新启动该程序"
    } else if cfg!(target_os = "linux") {
        "Linux 需要可用的 X11 DISPLAY；Wayland 下需要 uinput 权限或 libei/portal 支持"
    } else {
        ""
    }
}

fn key_name(k: &Key) -> String {
    match k {
        Key::Unicode(c) => c.to_string(),
        other => format!("{other:?}"),
    }
}

/// 供多处复用的双击间隔
pub const DOUBLE_CLICK_GAP: Duration = Duration::from_millis(DOUBLE_CLICK_GAP_MS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_button_parse() {
        assert_eq!(MouseButton::parse("Left").unwrap(), MouseButton::Left);
        assert_eq!(MouseButton::parse("中键").unwrap(), MouseButton::Middle);
        assert_eq!(MouseButton::parse("r").unwrap(), MouseButton::Right);
        assert!(MouseButton::parse("back").is_err());
    }

    #[test]
    fn scroll_dir_parse() {
        assert_eq!(ScrollDir::parse("UP").unwrap(), ScrollDir::Up);
        assert_eq!(ScrollDir::parse("向下").unwrap(), ScrollDir::Down);
        assert!(ScrollDir::parse("diagonal").is_err());
    }
}
