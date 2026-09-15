//! Linux X11 窗口控制

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window as XWindow,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::error::{DeskError, DeskResult};

fn connect() -> DeskResult<(x11rb::rust_connection::RustConnection, usize)> {
    x11rb::connect(None).map_err(|e| {
        DeskError::WindowOp(format!(
            "连接 X11 失败：{e}（请确认 DISPLAY 可用，当前仅支持 X11）"
        ))
    })
}

fn xid(id: u32) -> XWindow {
    id
}

pub fn focus(id: u32, _pid: u32) -> DeskResult<()> {
    let (conn, screen_num) = connect()?;
    let win = xid(id);
    let screen = &conn.setup().roots[screen_num];
    // 提升并设输入焦点
    conn.configure_window(
        win,
        &x11rb::protocol::xproto::ConfigureWindowAux::new().stack_mode(
            x11rb::protocol::xproto::StackMode::ABOVE,
        ),
    )
    .map_err(|e| DeskError::WindowOp(format!("XRaise 失败：{e}")))?;
    conn.set_input_focus(
        x11rb::protocol::xproto::InputFocus::POINTER_ROOT,
        win,
        x11rb::CURRENT_TIME,
    )
    .map_err(|e| DeskError::WindowOp(format!("SetInputFocus 失败：{e}")))?;

    // _NET_ACTIVE_WINDOW（EWMH）
    if let Ok(atom) = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW") {
        if let Ok(reply) = atom.reply() {
            let event = ClientMessageEvent::new(
                32,
                win,
                reply.atom,
                [2, x11rb::CURRENT_TIME, 0, 0, 0],
            );
            let _ = conn.send_event(
                false,
                screen.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            );
        }
    }
    conn.flush()
        .map_err(|e| DeskError::WindowOp(format!("X flush 失败：{e}")))?;
    let _ = COPY_DEPTH_FROM_PARENT;
    let _ = AtomEnum::WINDOW;
    Ok(())
}

pub fn set_bounds(id: u32, x: i32, y: i32, width: u32, height: u32) -> DeskResult<()> {
    let (conn, _) = connect()?;
    let win = xid(id);
    conn.configure_window(
        win,
        &x11rb::protocol::xproto::ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(width)
            .height(height),
    )
    .map_err(|e| DeskError::WindowOp(format!("XMoveResize 失败：{e}")))?;
    conn.flush()
        .map_err(|e| DeskError::WindowOp(format!("X flush 失败：{e}")))?;
    Ok(())
}

fn change_wm_state(id: u32, add: bool, atom_name: &[u8]) -> DeskResult<()> {
    let (conn, screen_num) = connect()?;
    let win = xid(id);
    let screen = &conn.setup().roots[screen_num];
    let net_wm_state = conn
        .intern_atom(false, b"_NET_WM_STATE")
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .reply()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .atom;
    let state_atom = conn
        .intern_atom(false, atom_name)
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .reply()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .atom;
    let action = if add { 1u32 } else { 0u32 };
    let event = ClientMessageEvent::new(32, win, net_wm_state, [action, state_atom, 0, 0, 0]);
    conn.send_event(
        false,
        screen.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    conn.flush()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    Ok(())
}

pub fn minimize(id: u32) -> DeskResult<()> {
    let (conn, screen_num) = connect()?;
    let win = xid(id);
    let screen = &conn.setup().roots[screen_num];
    let atom = conn
        .intern_atom(false, b"WM_CHANGE_STATE")
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .reply()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .atom;
    // IconicState = 3
    let event = ClientMessageEvent::new(32, win, atom, [3, 0, 0, 0, 0]);
    conn.send_event(
        false,
        screen.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    conn.flush()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    Ok(())
}

pub fn maximize(id: u32) -> DeskResult<()> {
    change_wm_state(id, true, b"_NET_WM_STATE_MAXIMIZED_VERT")?;
    change_wm_state(id, true, b"_NET_WM_STATE_MAXIMIZED_HORZ")
}

pub fn restore(id: u32) -> DeskResult<()> {
    change_wm_state(id, false, b"_NET_WM_STATE_MAXIMIZED_VERT")?;
    change_wm_state(id, false, b"_NET_WM_STATE_MAXIMIZED_HORZ")?;
    change_wm_state(id, false, b"_NET_WM_STATE_HIDDEN")?;
    // Map raised
    let (conn, _) = connect()?;
    conn.map_window(xid(id))
        .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    conn.flush()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    Ok(())
}

pub fn close(id: u32) -> DeskResult<()> {
    let (conn, screen_num) = connect()?;
    let win = xid(id);
    let screen = &conn.setup().roots[screen_num];
    let atom = conn
        .intern_atom(false, b"_NET_CLOSE_WINDOW")
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .reply()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?
        .atom;
    let event = ClientMessageEvent::new(32, win, atom, [x11rb::CURRENT_TIME, 0, 0, 0, 0]);
    conn.send_event(
        false,
        screen.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    conn.flush()
        .map_err(|e| DeskError::WindowOp(e.to_string()))?;
    Ok(())
}
