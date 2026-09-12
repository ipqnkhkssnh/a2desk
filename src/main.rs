//! a2desk —— 跨平台桌面控制 MCP 服务器（stdio 传输）

use std::process::ExitCode;

use a2desk::server::A2DeskServer;
use a2desk::Config;
use rmcp::transport::stdio;
use rmcp::ServiceExt;

#[derive(Debug, Default)]
struct Cli {
    help: bool,
    version: bool,
    selftest: bool,
    verbose: bool,
    no_permission_prompt: bool,
    log_level: Option<String>,
}

fn print_help() {
    println!(
        r#"a2desk {ver} —— 跨平台桌面控制 MCP 服务器

用法:
  a2desk [选项]              以 MCP stdio 服务器方式运行（供 MCP 客户端调用）
  a2desk --selftest          自检屏幕/截屏/输入/应用枚举是否可用

选项:
  -h, --help                 显示帮助
  -V, --version              显示版本
      --selftest             运行自检
  -v, --verbose              输出调试日志（写到 stderr）
      --log-level <LEVEL>    日志级别: error|warn|info|debug|trace（默认 info）
      --no-permission-prompt 缺少权限时不弹系统授权提示（默认会弹一次）

环境变量:
  A2DESK_LOG                 等同于 --log-level

说明:
  本程序只使用 stdout 与 MCP 客户端通信，所有日志都写到 stderr。
  工具列表: list_screens / screenshot / mouse_move / mouse_click / mouse_double_click /
            mouse_drag / mouse_scroll / mouse_position / keyboard_type / keyboard_press /
            keyboard_key_down / keyboard_key_up / list_apps
"#,
        ver = env!("CARGO_PKG_VERSION")
    );
}

fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut it = args.peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => cli.help = true,
            "-V" | "--version" => cli.version = true,
            "--selftest" => cli.selftest = true,
            "-v" | "--verbose" => cli.verbose = true,
            "--no-permission-prompt" => cli.no_permission_prompt = true,
            "--log-level" => {
                let v = it.next().ok_or("--log-level 需要一个值")?;
                cli.log_level = Some(v);
            }
            other if other.starts_with("--log-level=") => {
                cli.log_level = Some(other["--log-level=".len()..].to_string());
            }
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(cli)
}

fn init_tracing(cli: &Cli) {
    let level = cli
        .log_level
        .clone()
        .or_else(|| std::env::var("A2DESK_LOG").ok())
        .unwrap_or_else(|| {
            if cli.verbose {
                "debug".into()
            } else {
                "info".into()
            }
        });

    let filter = tracing_subscriber::EnvFilter::try_new(&level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // 关键：日志必须写 stderr，stdout 是 MCP 协议通道
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("a2desk: {e}\n使用 --help 查看用法");
            return ExitCode::from(2);
        }
    };

    if cli.help {
        print_help();
        return ExitCode::SUCCESS;
    }
    if cli.version {
        println!("a2desk {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    init_tracing(&cli);

    // Windows: 让坐标体系统一为物理像素（与截屏、多显示器枚举保持一致）
    #[cfg(target_os = "windows")]
    {
        let _ = enigo::set_dpi_awareness();
    }

    let cfg = Config {
        prompt_for_permission: !cli.no_permission_prompt,
    };

    if cli.selftest {
        return a2desk::selftest::run(cfg).await;
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "a2desk MCP server 启动 (stdio)"
    );

    let server = A2DeskServer::new(cfg);
    match server.serve(stdio()).await {
        Ok(service) => {
            match service.waiting().await {
                Ok(reason) => tracing::info!(?reason, "服务结束"),
                Err(e) => {
                    eprintln!("a2desk: 连接异常结束：{e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("a2desk: MCP 服务启动失败：{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok() {
        let cli = parse_args(
            ["--selftest", "-v", "--no-permission-prompt"]
                .iter()
                .map(|s| s.to_string()),
        )
        .unwrap();
        assert!(cli.selftest && cli.verbose && cli.no_permission_prompt);
    }

    #[test]
    fn parse_log_level() {
        let cli = parse_args(["--log-level=trace"].iter().map(|s| s.to_string())).unwrap();
        assert_eq!(cli.log_level.as_deref(), Some("trace"));
    }

    #[test]
    fn parse_unknown() {
        assert!(parse_args(["--nope"].iter().map(|s| s.to_string())).is_err());
    }
}
