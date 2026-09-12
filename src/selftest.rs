//! `--selftest`：不依赖 MCP 客户端，直接自检各模块是否可用。

use std::process::ExitCode;

use serde_json::json;

use crate::apps::{self, AppQuery};
use crate::input::InputHandle;
use crate::screens::{self, CaptureOptions, OutFormat};
use crate::Config;

pub async fn run(cfg: Config) -> ExitCode {
    let mut failures: Vec<String> = Vec::new();

    // 1. 屏幕
    println!("== 屏幕 ==");
    let screens = match screens::all_screens() {
        Ok(s) => {
            for m in &s {
                println!(
                    "  [{}] {} {}x{} @({},{}) scale={} primary={} builtin={}",
                    m.index,
                    m.info.name,
                    m.info.width,
                    m.info.height,
                    m.info.x,
                    m.info.y,
                    m.info.scale_factor,
                    m.info.is_primary,
                    m.info.is_builtin
                );
            }
            Some(s)
        }
        Err(e) => {
            println!("  失败：{e}");
            failures.push(format!("屏幕枚举失败：{e}"));
            None
        }
    };

    // 2. 截屏
    println!("== 截屏 ==");
    if let Some(list) = &screens {
        if let Some(primary) = list.iter().find(|s| s.info.is_primary).or_else(|| list.first()) {
            let opts = CaptureOptions {
                region: None,
                scale: 0.25,
                max_width: Some(640),
                format: OutFormat::Jpeg,
                quality: 70,
            };
            match screens::capture(primary, &opts) {
                Ok((bytes, meta)) => {
                    println!(
                        "  成功：{}x{} pixel_ratio={:.3} {} 字节 {}",
                        meta.image_width,
                        meta.image_height,
                        meta.pixel_ratio,
                        meta.byte_size,
                        meta.format
                    );
                    if meta.byte_size < 1024 {
                        println!("  警告：图片小于 1KB，可能是权限不足导致的空白/黑屏截图");
                    }
                    let _ = bytes;
                }
                Err(e) => {
                    println!("  失败：{e}");
                    failures.push(format!("截屏失败：{e}"));
                }
            }

            // 局部区域截屏
            let opts = CaptureOptions {
                region: Some(crate::types::Region {
                    x: 0,
                    y: 0,
                    width: 80.min(primary.info.width),
                    height: 40.min(primary.info.height),
                }),
                scale: 1.0,
                max_width: None,
                format: OutFormat::Jpeg,
                quality: 85,
            };
            match screens::capture(primary, &opts) {
                Ok((bytes, meta)) => println!(
                    "  区域截图成功：{}x{} {} 字节",
                    meta.image_width,
                    meta.image_height,
                    bytes.len()
                ),
                Err(e) => {
                    println!("  区域截图失败：{e}");
                    failures.push(format!("区域截屏失败：{e}"));
                }
            }
        }
    }

    // 3. 输入（需要系统权限）
    println!("== 输入控制 ==");
    let input = InputHandle::spawn(cfg.prompt_for_permission);
    match input.position().await {
        Ok((x, y)) => println!("  鼠标当前位置：({x}, {y})"),
        Err(e) => println!("  不可用（不影响截屏/枚举工具）：{e}"),
    }

    // 4. 应用枚举
    println!("== 应用 ==");
    let query = AppQuery {
        limit: 5,
        ..Default::default()
    };
    match apps::list_apps(&query) {
        Ok(list) => {
            println!(
                "  进程总数（匹配）：{}，返回 {}（窗口信息可用：{}）",
                list.total_matched,
                list.apps.len(),
                list.windows_available
            );
            for a in &list.apps {
                println!(
                    "  pid={} {} cpu={:.1}% mem={}MB windows={}",
                    a.pid,
                    a.name,
                    a.cpu_usage,
                    a.memory_bytes / 1024 / 1024,
                    a.windows.len()
                );
            }
            for w in &list.warnings {
                println!("  警告：{w}");
            }
        }
        Err(e) => {
            println!("  失败：{e}");
            failures.push(format!("应用枚举失败：{e}"));
        }
    }

    println!();
    if failures.is_empty() {
        println!("self-test 通过 ✅");
        report_hint();
        ExitCode::SUCCESS
    } else {
        println!("self-test 存在问题：");
        for f in &failures {
            println!("  - {f}");
        }
        report_hint();
        ExitCode::FAILURE
    }
}

fn report_hint() {
    let hint = json!({
        "macos": "截屏需要「屏幕录制」权限；鼠标键盘需要「辅助功能」权限（两者彼此独立，只勾「屏幕录制」无法模拟输入）。在 系统设置 → 隐私与安全性 中为运行 a2desk 的程序（终端/MCP 客户端本体）勾选后，完全退出并重新启动该程序（TCC 会在进程内缓存判定结果）。",
        "linux": "X11 需要 DISPLAY；Wayland 截屏依赖 portal，输入模拟依赖 uinput/libei。",
        "windows": "若以服务方式运行需勾选「允许服务与桌面交互」，否则请在用户会话中运行。"
    });
    let key = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "windows"
    };
    println!("权限提示（{}）：{}", key, hint[key]);
}

