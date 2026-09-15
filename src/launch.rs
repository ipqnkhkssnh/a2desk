//! 启动应用（按路径 / 开始菜单 / 应用名）

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::error::{DeskError, DeskResult};
use crate::types::WindowInfo;
use crate::windows::{self, WindowSelector};

/// 启动参数
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    /// 可执行路径、.lnk、.app、应用名或开始菜单项关键字
    pub target: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// 启动后是否等待窗口
    pub wait_window: bool,
    /// 等待窗口时的 query（默认用 target 文件名）
    pub window_query: Option<String>,
    /// 等到窗口后放到哪块屏幕
    pub screen: Option<String>,
    pub timeout_ms: u64,
}

/// 启动结果
#[derive(Debug, Clone, Serialize)]
pub struct LaunchResult {
    pub action: &'static str,
    pub target: String,
    pub resolved_path: Option<String>,
    pub pid: Option<u32>,
    pub window: Option<WindowInfo>,
}

/// 启动应用；可选 wait_for_window + set_window_screen
pub fn launch_app(req: &LaunchRequest) -> DeskResult<LaunchResult> {
    let target = req.target.trim();
    if target.is_empty() {
        return Err(DeskError::InvalidArgument("target 不能为空".into()));
    }
    if target.contains('\0') {
        return Err(DeskError::InvalidArgument("target 不能包含空字符".into()));
    }

    let (resolved, child_pid) = launch_target(target, &req.args, req.cwd.as_deref())?;

    let mut window = None;
    if req.wait_window {
        let query = req
            .window_query
            .clone()
            .unwrap_or_else(|| default_window_query(target, resolved.as_deref()));
        // 给进程一点启动时间（start/.lnk 的 child pid 往往是壳进程，等待只靠 query）
        std::thread::sleep(Duration::from_millis(400));
        let sel = WindowSelector {
            query: Some(query),
            ..Default::default()
        };
        let win = windows::wait_for_window(&sel, req.timeout_ms, 250)?;
        if let Some(screen) = req.screen.as_deref() {
            let placed = windows::set_window_screen(
                &WindowSelector {
                    id: Some(win.id),
                    ..Default::default()
                },
                Some(screen),
                40,
            )?;
            window = Some(placed);
        } else {
            window = Some(win);
        }
    }

    Ok(LaunchResult {
        action: "launch_app",
        target: target.to_string(),
        resolved_path: resolved,
        pid: child_pid,
        window,
    })
}

fn default_window_query(target: &str, resolved: Option<&str>) -> String {
    let path = resolved
        .map(Path::new)
        .unwrap_or_else(|| Path::new(target));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(target)
        .to_string();
    // 同时带上原 target，方便 "优优|YouYou"
    if stem.eq_ignore_ascii_case(target) {
        stem
    } else {
        format!("{stem}|{target}")
    }
}

fn launch_target(
    target: &str,
    args: &[String],
    cwd: Option<&Path>,
) -> DeskResult<(Option<String>, Option<u32>)> {
    let path = PathBuf::from(target);
    if path.exists() {
        return spawn_path(&path, args, cwd);
    }

    // 平台解析应用名 / 开始菜单 / .desktop
    if let Some(resolved) = resolve_app_name(target)? {
        return spawn_resolved(&resolved, args, cwd);
    }

    Err(DeskError::InvalidArgument(format!(
        "无法解析启动目标 `{target}`：不是已有路径，也未在开始菜单/应用列表中找到。\
         可传绝对路径、.lnk/.app、或应用显示名。"
    )))
}

#[derive(Debug)]
#[allow(dead_code)] // macOS / Linux 变体在 Windows 构建中不会构造
enum ResolvedTarget {
    Path(PathBuf),
    /// macOS `open -a Name`
    MacAppName(String),
    /// Linux desktop id for gtk-launch
    DesktopId(String),
}

fn spawn_resolved(
    resolved: &ResolvedTarget,
    args: &[String],
    cwd: Option<&Path>,
) -> DeskResult<(Option<String>, Option<u32>)> {
    match resolved {
        ResolvedTarget::Path(p) => spawn_path(p, args, cwd),
        ResolvedTarget::MacAppName(name) => {
            let mut cmd = Command::new("open");
            cmd.arg("-a").arg(name);
            if !args.is_empty() {
                cmd.arg("--args");
                cmd.args(args);
            }
            if let Some(cwd) = cwd {
                cmd.current_dir(cwd);
            }
            let child = cmd
                .spawn()
                .map_err(|e| DeskError::SystemInfo(format!("open -a 启动失败：{e}")))?;
            Ok((Some(format!("app:{name}")), Some(child.id())))
        }
        ResolvedTarget::DesktopId(id) => {
            let mut cmd = Command::new("gtk-launch");
            cmd.arg(id);
            cmd.args(args);
            if let Some(cwd) = cwd {
                cmd.current_dir(cwd);
            }
            match cmd.spawn() {
                Ok(child) => Ok((Some(format!("desktop:{id}")), Some(child.id()))),
                Err(_) => {
                    // 回退：直接找 Exec=
                    Err(DeskError::SystemInfo(format!(
                        "gtk-launch `{id}` 失败，请改用可执行文件绝对路径"
                    )))
                }
            }
        }
    }
}

fn spawn_path(
    path: &Path,
    args: &[String],
    cwd: Option<&Path>,
) -> DeskResult<(Option<String>, Option<u32>)> {
    let path_str = path.display().to_string();
    #[cfg(windows)]
    {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "lnk" || ext == "url" {
            // start 对 .lnk 最稳妥
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "start", "", &path_str]);
            cmd.args(args);
            if let Some(cwd) = cwd {
                cmd.current_dir(cwd);
            }
            let child = cmd
                .spawn()
                .map_err(|e| DeskError::SystemInfo(format!("启动快捷方式失败：{e}")))?;
            return Ok((Some(path_str), Some(child.id())));
        }
    }

    let mut cmd = Command::new(path);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let child = cmd
        .spawn()
        .map_err(|e| DeskError::SystemInfo(format!("启动进程失败：{e}")))?;
    Ok((Some(path_str), Some(child.id())))
}

fn resolve_app_name(name: &str) -> DeskResult<Option<ResolvedTarget>> {
    #[cfg(windows)]
    {
        return Ok(resolve_windows_start_menu(name));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = name;
        // open -a 可直接按应用名启动
        return Ok(Some(ResolvedTarget::MacAppName(name.to_string())));
    }
    #[cfg(target_os = "linux")]
    {
        return Ok(resolve_linux_desktop(name));
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = name;
        Ok(None)
    }
}

#[cfg(windows)]
fn resolve_windows_start_menu(name: &str) -> Option<ResolvedTarget> {
    let needle = name.to_lowercase();
    let mut roots = Vec::new();
    if let Ok(pd) = std::env::var("ProgramData") {
        roots.push(PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(appdata) = std::env::var("AppData") {
        roots.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }

    let mut best: Option<(i32, PathBuf)> = None;
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let walker = walkdir_lnk(&root);
        for p in walker {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let score = if stem == needle {
                100
            } else if stem.starts_with(&needle) || needle.starts_with(&stem) {
                80
            } else if stem.contains(&needle) || needle.contains(&stem) {
                60
            } else {
                continue;
            };
            if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                best = Some((score, p));
            }
        }
    }
    best.map(|(_, p)| ResolvedTarget::Path(p))
}

#[cfg(windows)]
fn walkdir_lnk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("lnk"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn resolve_linux_desktop(name: &str) -> Option<ResolvedTarget> {
    let needle = name.to_lowercase();
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/local/share/applications"));

    let mut best: Option<(i32, String, Option<PathBuf>)> = None;
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let content = std::fs::read_to_string(&p).unwrap_or_default();
            let display = content
                .lines()
                .find(|l| l.starts_with("Name="))
                .map(|l| l.trim_start_matches("Name=").to_lowercase())
                .unwrap_or_default();
            let score = if id.to_lowercase() == needle || display == needle {
                100
            } else if id.to_lowercase().contains(&needle) || display.contains(&needle) {
                70
            } else {
                continue;
            };
            // Prefer Exec= absolute binary if present
            let exec = content
                .lines()
                .find(|l| l.starts_with("Exec="))
                .map(|l| l.trim_start_matches("Exec=").split_whitespace().next().unwrap_or(""))
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            if best.as_ref().map(|(s, _, _)| score > *s).unwrap_or(true) {
                best = Some((score, id, exec.filter(|e| e.exists())));
            }
        }
    }
    match best {
        Some((_, _, Some(exec))) => Some(ResolvedTarget::Path(exec)),
        Some((_, id, None)) => Some(ResolvedTarget::DesktopId(id)),
        None => None,
    }
}
