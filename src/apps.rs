//! 运行中的应用（进程）与窗口枚举

use std::collections::HashMap;

use sysinfo::{ProcessesToUpdate, System};

use crate::error::{DeskError, DeskResult};
use crate::types::AppInfo;
use crate::windows;

/// 排序字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppSort {
    Cpu,
    Memory,
    Pid,
    Name,
    StartTime,
}

impl AppSort {
    pub fn parse(s: &str) -> DeskResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "cpu" => Ok(AppSort::Cpu),
            "memory" | "mem" | "ram" => Ok(AppSort::Memory),
            "pid" => Ok(AppSort::Pid),
            "name" | "title" => Ok(AppSort::Name),
            "start_time" | "start" | "uptime" => Ok(AppSort::StartTime),
            other => Err(DeskError::InvalidArgument(format!(
                "不支持的排序字段 `{other}`，可选：cpu、memory、pid、name、start_time"
            ))),
        }
    }
}

/// 查询参数
#[derive(Debug, Clone)]
pub struct AppQuery {
    /// 名称/路径/命令/窗口标题的子串过滤，可用 `|` 分隔多个（任一命中即可）
    pub filter: Option<String>,
    /// 只保留这些 pid
    pub pids: Option<Vec<u32>>,
    /// 是否附带窗口信息
    pub include_windows: bool,
    /// 只返回拥有窗口的进程
    pub only_with_windows: bool,
    pub sort_by: AppSort,
    /// 最多返回多少条（1..=2000）
    pub limit: usize,
}

impl Default for AppQuery {
    fn default() -> Self {
        Self {
            filter: None,
            pids: None,
            include_windows: true,
            only_with_windows: false,
            sort_by: AppSort::Cpu,
            limit: 50,
        }
    }
}

/// 查询结果
#[derive(Debug, Clone)]
pub struct AppList {
    pub apps: Vec<AppInfo>,
    /// 过滤后命中的进程总数（未截断前）
    pub total_matched: usize,
    /// 是否因为 limit 被截断
    pub truncated: bool,
    /// 窗口信息是否可用
    pub windows_available: bool,
    pub warnings: Vec<String>,
}

/// 枚举进程（可选带窗口信息）
pub fn list_apps(q: &AppQuery) -> DeskResult<AppList> {
    let mut sys = System::new_all();
    // CPU 使用率需要两次采样才有意义
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut warnings: Vec<String> = Vec::new();
    let (windows_by_pid, windows_available) = if q.include_windows {
        match windows::collect_windows_by_pid() {
            Ok(map) => (map, true),
            Err(e) => {
                warnings.push(e.to_string());
                (HashMap::new(), false)
            }
        }
    } else {
        (HashMap::new(), false)
    };

    let mut apps: Vec<AppInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let pid_u32 = pid.as_u32();
            AppInfo {
                pid: pid_u32,
                name: p.name().to_string_lossy().into_owned(),
                exe: p.exe().map(|e| e.display().to_string()),
                cmd: p
                    .cmd()
                    .iter()
                    .map(|c| c.to_string_lossy().into_owned())
                    .collect(),
                parent_pid: p.parent().map(|pp| pp.as_u32()),
                status: format!("{:?}", p.status()),
                cpu_usage: p.cpu_usage(),
                memory_bytes: p.memory(),
                start_time: p.start_time(),
                windows: windows_by_pid.get(&pid_u32).cloned().unwrap_or_default(),
            }
        })
        .collect();

    if let Some(pids) = &q.pids {
        apps.retain(|a| pids.contains(&a.pid));
    }

    if let Some(filter) = q.filter.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        let needles: Vec<String> = filter
            .split('|')
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        if !needles.is_empty() {
            apps.retain(|a| {
                needles.iter().any(|n| app_matches(a, n))
            });
        }
    }

    if q.only_with_windows {
        apps.retain(|a| !a.windows.is_empty());
    }

    match q.sort_by {
        AppSort::Cpu => apps.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
        }),
        AppSort::Memory => apps.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes)),
        AppSort::Pid => apps.sort_by_key(|a| a.pid),
        AppSort::Name => apps.sort_by_key(|a| a.name.to_lowercase()),
        AppSort::StartTime => apps.sort_by_key(|a| std::cmp::Reverse(a.start_time)),
    }

    let total_matched = apps.len();
    let limit = if q.limit == 0 { 2000 } else { q.limit.clamp(1, 2000) };
    let truncated = total_matched > limit;
    apps.truncate(limit);

    Ok(AppList {
        apps,
        total_matched,
        truncated,
        windows_available,
        warnings,
    })
}

fn app_matches(a: &AppInfo, needle: &str) -> bool {
    if a.name.to_lowercase().contains(needle) {
        return true;
    }
    if let Some(exe) = &a.exe {
        if exe.to_lowercase().contains(needle) {
            return true;
        }
    }
    if a.cmd.iter().any(|c| c.to_lowercase().contains(needle)) {
        return true;
    }
    a.windows.iter().any(|w| {
        w.title.to_lowercase().contains(needle) || w.app_name.to_lowercase().contains(needle)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_parse() {
        assert_eq!(AppSort::parse("cpu").unwrap(), AppSort::Cpu);
        assert_eq!(AppSort::parse("MEM").unwrap(), AppSort::Memory);
        assert!(AppSort::parse("gpu").is_err());
    }

    #[test]
    fn filter_matches_name() {
        let app = AppInfo {
            pid: 1,
            name: "Google Chrome".into(),
            exe: Some("/Applications/Google Chrome.app".into()),
            cmd: vec!["chrome".into()],
            parent_pid: None,
            status: "Run".into(),
            cpu_usage: 0.0,
            memory_bytes: 0,
            start_time: 0,
            windows: vec![],
        };
        assert!(app_matches(&app, "chrome"));
        assert!(app_matches(&app, "google"));
        assert!(!app_matches(&app, "firefox"));
    }
}
