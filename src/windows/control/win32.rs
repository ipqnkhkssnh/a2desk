//! Windows 窗口控制（Win32）

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, GetForegroundWindow, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsZoomed, PostMessageW, SetForegroundWindow,
    SetWindowPos, ShowWindow, ASFW_ANY, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CLOSE,
};

use crate::error::{DeskError, DeskResult};

fn hwnd_from_id(id: u32) -> HWND {
    id as isize as HWND
}

fn ensure_window(hwnd: HWND) -> DeskResult<()> {
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err(DeskError::WindowNotFound(format!(
            "句柄无效（id={}），可能窗口已关闭或 HWND 超出 u32 截断范围",
            hwnd as usize
        )));
    }
    Ok(())
}

pub fn focus(id: u32, _pid: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;

    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);

        // 最小化时必须 restore，并稍等再抢焦点
        let iconic = IsIconic(hwnd) != 0;
        if iconic {
            ShowWindow(hwnd, SW_RESTORE);
            std::thread::sleep(std::time::Duration::from_millis(120));
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }

        let fg = GetForegroundWindow();
        let mut fg_tid = 0u32;
        let mut target_tid = 0u32;
        if !fg.is_null() {
            GetWindowThreadProcessId(fg, &mut fg_tid);
        }
        GetWindowThreadProcessId(hwnd, &mut target_tid);

        use windows_sys::Win32::System::Threading::AttachThreadInput;
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        let cur = GetCurrentThreadId();
        if fg_tid != 0 && fg_tid != cur {
            AttachThreadInput(cur, fg_tid, TRUE);
        }
        if target_tid != 0 && target_tid != cur {
            AttachThreadInput(cur, target_tid, TRUE);
        }

        BringWindowToTop(hwnd);
        let ok = SetForegroundWindow(hwnd);

        if fg_tid != 0 && fg_tid != cur {
            AttachThreadInput(cur, fg_tid, 0);
        }
        if target_tid != 0 && target_tid != cur {
            AttachThreadInput(cur, target_tid, 0);
        }

        SetWindowPos(
            hwnd,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );

        // 二次确认：若仍最小化再 restore 一次
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }

        if ok == 0 {
            tracing::warn!("SetForegroundWindow 返回 false，已尽量置顶");
        }
    }
    Ok(())
}

pub fn set_bounds(id: u32, x: i32, y: i32, width: u32, height: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    let ok = unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            x,
            y,
            width as i32,
            height as i32,
            SWP_SHOWWINDOW | SWP_NOACTIVATE,
        )
    };
    if ok == 0 {
        return Err(DeskError::WindowOp(format!(
            "SetWindowPos 失败（id={id}, x={x}, y={y}, {}x{}）",
            width, height
        )));
    }
    Ok(())
}

pub fn minimize(id: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
    Ok(())
}

pub fn maximize(id: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        ShowWindow(hwnd, SW_MAXIMIZE);
    }
    Ok(())
}

pub fn restore(id: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    unsafe { ShowWindow(hwnd, SW_RESTORE) };
    Ok(())
}

pub fn close(id: u32) -> DeskResult<()> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    let ok = unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    if ok == 0 {
        return Err(DeskError::WindowOp(format!("发送 WM_CLOSE 失败（id={id}）")));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn get_rect(id: u32) -> DeskResult<(i32, i32, u32, u32)> {
    let hwnd = hwnd_from_id(id);
    ensure_window(hwnd)?;
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { GetWindowRect(hwnd, &mut rc) };
    if ok == 0 {
        return Err(DeskError::WindowOp("GetWindowRect 失败".into()));
    }
    Ok((
        rc.left,
        rc.top,
        (rc.right - rc.left).max(0) as u32,
        (rc.bottom - rc.top).max(0) as u32,
    ))
}

pub fn is_minimized(id: u32) -> bool {
    let hwnd = hwnd_from_id(id);
    unsafe { IsWindow(hwnd) != 0 && IsIconic(hwnd) != 0 }
}

#[allow(dead_code)]
pub fn is_maximized(id: u32) -> bool {
    let hwnd = hwnd_from_id(id);
    unsafe { IsWindow(hwnd) != 0 && IsZoomed(hwnd) != 0 }
}

// 抑制未使用导入告警（部分仅调试用）
const _: BOOL = TRUE;
const _: LPARAM = 0;
