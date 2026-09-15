//! macOS 窗口控制（NSRunningApplication + Accessibility 位置/尺寸）

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_graphics::geometry::{CGPoint, CGSize};
use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::MainThreadMarker;

use crate::error::{DeskError, DeskResult};

type AXUIElementRef = *mut c_void;
type AXError = i32;
type CFTypeRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFTypeRef) -> AXError;
    fn AXValueCreate(value_type: u32, value_ptr: *const c_void) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

const K_AX_ERROR_SUCCESS: AXError = 0;
const K_AX_VALUE_CG_POINT: u32 = 1;
const K_AX_VALUE_CG_SIZE: u32 = 2;

fn ax_str(name: &str) -> CFString {
    CFString::new(name)
}

/// 用 CGWindowID 找到 pid 对应应用的匹配窗口 AX 元素较复杂；
/// 这里用 pid 激活应用，再用 AX 主窗口设置位置/尺寸。
fn front_window_for_pid(pid: u32) -> DeskResult<AXUIElementRef> {
    unsafe {
        let app = AXUIElementCreateApplication(pid as i32);
        if app.is_null() {
            return Err(DeskError::WindowOp(format!(
                "无法创建 AX 应用对象（pid={pid}），请确认已授予辅助功能权限"
            )));
        }
        let mut win: CFTypeRef = std::ptr::null();
        let attr = ax_str("AXFocusedWindow");
        let err = AXUIElementCopyAttributeValue(app, attr.as_CFTypeRef(), &mut win);
        if err != K_AX_ERROR_SUCCESS || win.is_null() {
            let attr2 = ax_str("AXMainWindow");
            let err2 = AXUIElementCopyAttributeValue(app, attr2.as_CFTypeRef(), &mut win);
            if err2 != K_AX_ERROR_SUCCESS || win.is_null() {
                CFRelease(app as CFTypeRef);
                return Err(DeskError::WindowOp(format!(
                    "无法获取应用主窗口（pid={pid}），err={err}/{err2}"
                )));
            }
        }
        // app 仍由系统持有引用；这里只返回 window
        let _ = app;
        Ok(win as AXUIElementRef)
    }
}

pub fn focus(id: u32, pid: u32) -> DeskResult<()> {
    let _ = id;
    // NSRunningApplication::activate
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) else {
        return Err(DeskError::WindowOp(format!(
            "找不到 pid={pid} 的运行中应用"
        )));
    };
    // activateIgnoringOtherApps
    let ok = unsafe { app.activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::from_bits_truncate(1 << 1)) };
    if !ok {
        tracing::warn!("activateWithOptions 返回 false（pid={pid}）");
    }
    let _ = Retained::into_raw(app);
    let _ = MainThreadMarker::new();
    Ok(())
}

pub fn set_bounds(id: u32, x: i32, y: i32, width: u32, height: u32) -> DeskResult<()> {
    // 通过枚举窗口拿 pid：调用方已有 pid，但这里只有 id。
    // 使用 list 里同 id 的窗口——控制层应传 pid。为兼容签名，从 xcap 再查一次。
    let pid = pid_for_window_id(id)?;
    let win = front_window_for_pid(pid)?;
    unsafe {
        let pos = CGPoint {
            x: x as f64,
            y: y as f64,
        };
        let size = CGSize {
            width: width as f64,
            height: height as f64,
        };
        let pos_val = AXValueCreate(K_AX_VALUE_CG_POINT, &pos as *const _ as *const c_void);
        let size_val = AXValueCreate(K_AX_VALUE_CG_SIZE, &size as *const _ as *const c_void);
        if pos_val.is_null() || size_val.is_null() {
            return Err(DeskError::WindowOp("AXValueCreate 失败".into()));
        }
        let pos_attr = ax_str("AXPosition");
        let size_attr = ax_str("AXSize");
        let e1 = AXUIElementSetAttributeValue(win, pos_attr.as_CFTypeRef(), pos_val);
        let e2 = AXUIElementSetAttributeValue(win, size_attr.as_CFTypeRef(), size_val);
        CFRelease(pos_val);
        CFRelease(size_val);
        CFRelease(win as CFTypeRef);
        if e1 != K_AX_ERROR_SUCCESS || e2 != K_AX_ERROR_SUCCESS {
            return Err(DeskError::WindowOp(format!(
                "设置窗口位置/尺寸失败（AX err={e1}/{e2}），请确认辅助功能权限"
            )));
        }
    }
    Ok(())
}

pub fn minimize(id: u32) -> DeskResult<()> {
    let pid = pid_for_window_id(id)?;
    let win = front_window_for_pid(pid)?;
    unsafe {
        let action = ax_str("AXMinimizeWindow");
        // 部分系统用 AXPress on minimize button；先试 perform action
        let err = AXUIElementPerformAction(win, action.as_CFTypeRef());
        CFRelease(win as CFTypeRef);
        if err != K_AX_ERROR_SUCCESS {
            // 退化为设置 AXMinimized
            let win2 = front_window_for_pid(pid)?;
            let attr = ax_str("AXMinimized");
            // 使用 kCFBooleanTrue
            extern "C" {
                static kCFBooleanTrue: CFTypeRef;
            }
            let e = AXUIElementSetAttributeValue(win2, attr.as_CFTypeRef(), kCFBooleanTrue);
            CFRelease(win2 as CFTypeRef);
            if e != K_AX_ERROR_SUCCESS {
                return Err(DeskError::WindowOp(format!("最小化失败 err={err}/{e}")));
            }
        }
    }
    Ok(())
}

pub fn maximize(id: u32) -> DeskResult<()> {
    let pid = pid_for_window_id(id)?;
    let win = front_window_for_pid(pid)?;
    unsafe {
        let attr = ax_str("AXFullScreen");
        extern "C" {
            static kCFBooleanTrue: CFTypeRef;
        }
        let e = AXUIElementSetAttributeValue(win, attr.as_CFTypeRef(), kCFBooleanTrue);
        CFRelease(win as CFTypeRef);
        if e != K_AX_ERROR_SUCCESS {
            // 退回 zoom
            let win2 = front_window_for_pid(pid)?;
            let action = ax_str("AXZoomWindow");
            let e2 = AXUIElementPerformAction(win2, action.as_CFTypeRef());
            CFRelease(win2 as CFTypeRef);
            if e2 != K_AX_ERROR_SUCCESS {
                return Err(DeskError::WindowOp(format!("最大化失败 err={e}/{e2}")));
            }
        }
    }
    Ok(())
}

pub fn restore(id: u32) -> DeskResult<()> {
    let pid = pid_for_window_id(id)?;
    let win = front_window_for_pid(pid)?;
    unsafe {
        let attr = ax_str("AXMinimized");
        extern "C" {
            static kCFBooleanFalse: CFTypeRef;
        }
        let e = AXUIElementSetAttributeValue(win, attr.as_CFTypeRef(), kCFBooleanFalse);
        let attr2 = ax_str("AXFullScreen");
        let e2 = AXUIElementSetAttributeValue(win, attr2.as_CFTypeRef(), kCFBooleanFalse);
        CFRelease(win as CFTypeRef);
        if e != K_AX_ERROR_SUCCESS && e2 != K_AX_ERROR_SUCCESS {
            return Err(DeskError::WindowOp(format!("还原窗口失败 err={e}/{e2}")));
        }
    }
    Ok(())
}

pub fn close(id: u32) -> DeskResult<()> {
    let pid = pid_for_window_id(id)?;
    let win = front_window_for_pid(pid)?;
    unsafe {
        let action = ax_str("AXPress");
        // 优先找关闭按钮
        let mut btn: CFTypeRef = std::ptr::null();
        let close_attr = ax_str("AXCloseButton");
        let err = AXUIElementCopyAttributeValue(win, close_attr.as_CFTypeRef(), &mut btn);
        if err == K_AX_ERROR_SUCCESS && !btn.is_null() {
            let e = AXUIElementPerformAction(btn as AXUIElementRef, action.as_CFTypeRef());
            CFRelease(btn);
            CFRelease(win as CFTypeRef);
            if e != K_AX_ERROR_SUCCESS {
                return Err(DeskError::WindowOp(format!("关闭按钮 AXPress 失败 err={e}")));
            }
            return Ok(());
        }
        CFRelease(win as CFTypeRef);
    }
    Err(DeskError::WindowOp("无法定位关闭按钮".into()))
}

/// 平台侧最小化探测；上层已有 xcap.is_minimized，此处作补充
pub fn is_minimized(_id: u32) -> bool {
    false
}

fn pid_for_window_id(id: u32) -> DeskResult<u32> {
    use std::panic::AssertUnwindSafe;
    use xcap::Window;
    let windows = std::panic::catch_unwind(AssertUnwindSafe(Window::all))
        .map_err(|_| DeskError::SystemInfo("枚举窗口失败".into()))?
        .map_err(|e| DeskError::SystemInfo(format!("枚举窗口失败：{e}")))?;
    for w in windows {
        if w.id().unwrap_or(0) == id {
            return w
                .pid()
                .map_err(|e| DeskError::WindowOp(format!("读取窗口 pid 失败：{e}")));
        }
    }
    Err(DeskError::WindowNotFound(format!("找不到 id={id} 的窗口")))
}
