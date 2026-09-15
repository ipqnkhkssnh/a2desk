//! 真实的 MCP stdio 协议往返测试：启动编译好的 a2desk 二进制，
//! 完成 initialize → tools/list → tools/call 全流程。
//!
//! 只调用只读工具（list_screens / screenshot / list_apps / list_windows / clipboard_get），
//! 不会操作鼠标键盘或改动窗口。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};

struct Client {
    stdin: ChildStdin,
    rx: Receiver<String>,
    child: Child,
}

impl Client {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_a2desk"))
            .arg("--no-permission-prompt")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动 a2desk 失败");

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");

        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self { stdin, rx, child }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("写入请求失败");
        self.stdin.flush().expect("flush 失败");
    }

    fn next_message(&self, timeout: Duration) -> Value {
        let line = self
            .rx
            .recv_timeout(timeout)
            .unwrap_or_else(|_| panic!("等待响应超时（{timeout:?}）"));
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("响应不是合法 JSON: {e}\n{line}"))
    }

    /// 发请求并等待同 id 的响应（跳过通知/日志）
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        loop {
            let msg = self.next_message(Duration::from_secs(60));
            if msg.get("id").and_then(Value::as_u64) == Some(id) {
                return msg;
            }
        }
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 从 tools/call 结果里取出第一个文本内容并解析为 JSON
fn structured(result: &Value) -> Value {
    if let Some(sc) = result.pointer("/result/structuredContent") {
        if !sc.is_null() {
            return sc.clone();
        }
    }
    let text = result
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("结果里没有文本内容: {result}"));
    serde_json::from_str(text).expect("文本内容不是 JSON")
}

fn call(client: &mut Client, id: u64, tool: &str, args: Value) -> Value {
    client.request(
        id,
        "tools/call",
        json!({ "name": tool, "arguments": args }),
    )
}

/// 断言工具调用成功（isError 不为 true）
fn assert_not_error(result: &Value) {
    assert_ne!(
        result.pointer("/result/isError").and_then(Value::as_bool),
        Some(true),
        "工具返回了错误: {result}"
    );
}

/// 断言工具调用失败（isError == true）
fn assert_is_error(result: &Value) {
    assert_eq!(
        result.pointer("/result/isError").and_then(Value::as_bool),
        Some(true),
        "应当返回工具级错误: {result}"
    );
}

#[test]
fn mcp_stdio_roundtrip() {
    let mut client = Client::spawn();

    // 1. initialize
    let init = client.request(
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "a2desk-smoke", "version": "0.0.1" }
        }),
    );
    let server_name = init
        .pointer("/result/serverInfo/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert_eq!(server_name, "a2desk", "serverInfo.name 不对: {init}");
    assert!(
        init.pointer("/result/protocolVersion").is_some(),
        "initialize 结果缺少 protocolVersion: {init}"
    );

    client.notify("notifications/initialized");

    // 2. tools/list
    let tools = client.request(2, "tools/list", json!({}));
    let names: Vec<String> = tools
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("tools/list 结果异常: {tools}"))
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();

    for expected in [
        "list_screens",
        "screenshot",
        "mouse_move",
        "mouse_click",
        "mouse_double_click",
        "mouse_drag",
        "mouse_scroll",
        "mouse_position",
        "keyboard_type",
        "keyboard_press",
        "keyboard_key_down",
        "keyboard_key_up",
        "list_apps",
        "list_windows",
        "focus_window",
        "move_window",
        "resize_window",
        "set_window_bounds",
        "set_window_screen",
        "minimize_window",
        "maximize_window",
        "restore_window",
        "close_window",
        "screenshot_window",
        "wait_for_window",
        "find_text",
        "click_text",
        "wait_for_text",
        "type_in_window",
        "clipboard_get",
        "clipboard_set",
    ] {
        assert!(names.contains(&expected.to_string()), "缺少工具 {expected}，实际: {names:?}");
    }

    // 每个工具都要有 inputSchema
    for t in tools.pointer("/result/tools").unwrap().as_array().unwrap() {
        assert!(
            t.get("inputSchema").is_some(),
            "工具 {} 缺少 inputSchema",
            t.get("name").unwrap()
        );
    }

    // 3. list_screens
    let result = call(&mut client, 3, "list_screens", json!({}));
    assert_not_error(&result);
    let info = structured(&result);
    let screens = info["screens"].as_array().expect("screens 数组");
    assert!(!screens.is_empty(), "至少应有一块屏幕: {info}");
    for s in screens {
        assert!(s["width"].as_u64().unwrap_or(0) > 0);
        assert!(s["height"].as_u64().unwrap_or(0) > 0);
        assert!(s["index"].as_u64().is_some());
    }

    // 4. screenshot（限制宽度，避免测试产出巨大 base64）
    let result = call(
        &mut client,
        4,
        "screenshot",
        json!({ "scale": 1.0, "max_width": 320, "format": "jpeg", "quality": 70 }),
    );
    assert_not_error(&result);
    assert_eq!(
        result.pointer("/result/content/0/type").and_then(Value::as_str),
        Some("image"),
        "截图应返回 image content: {result}"
    );
    assert_eq!(
        result.pointer("/result/content/0/mimeType").and_then(Value::as_str),
        Some("image/jpeg")
    );
    let b64 = result
        .pointer("/result/content/0/data")
        .and_then(Value::as_str)
        .expect("base64 数据");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64 解码失败");
    assert!(bytes.len() > 1024, "截图数据太小: {} 字节", bytes.len());
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "不是 JPEG（缺少 SOI）");

    // 把截图落到 target/ 便于人工查看
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mcp-screenshot.jpg");
    std::fs::write(&out, &bytes).expect("写出截图失败");
    println!("截图已保存到 {}", out.display());

    let meta = structured(&result);
    assert!(meta["image_width"].as_u64().unwrap_or(0) <= 320);
    assert!(meta["pixel_ratio"].as_f64().unwrap_or(0.0) > 0.0);

    // 5. 区域截图 + 越界裁剪
    let result = call(
        &mut client,
        5,
        "screenshot",
        json!({
            "region": { "x": -10, "y": -10, "width": 60, "height": 40 },
            "format": "png"
        }),
    );
    assert_not_error(&result);
    let meta = structured(&result);
    assert_eq!(meta["region"]["x"].as_i64(), Some(0));
    assert_eq!(meta["region"]["y"].as_i64(), Some(0));
    assert_eq!(meta["region_clamped"].as_bool(), Some(true), "{meta}");

    // 6. list_apps：过滤本进程名，必然能命中
    let result = call(
        &mut client,
        6,
        "list_apps",
        json!({ "filter": "a2desk", "limit": 10, "include_windows": true }),
    );
    assert_not_error(&result);
    let apps = structured(&result);
    assert!(
        apps["total_matched"].as_u64().unwrap_or(0) >= 1,
        "应至少匹配到 a2desk 自身: {apps}"
    );
    let first = &apps["apps"][0];
    assert!(first["pid"].as_u64().unwrap_or(0) > 0);
    assert!(first["name"].as_str().is_some());

    // 7. 参数错误应当返回工具级错误而不是崩溃
    let result = call(
        &mut client,
        7,
        "mouse_move",
        json!({ "screen": "999", "x": 1, "y": 1 }),
    );
    assert_is_error(&result);

    // 8. 未知按键名同样应返回错误
    let result = call(
        &mut client,
        8,
        "keyboard_press",
        json!({ "keys": ["nosuchkey"] }),
    );
    assert_is_error(&result);

    // 9. 非法滚动数量应被拒绝
    let result = call(
        &mut client,
        9,
        "mouse_scroll",
        json!({ "direction": "down", "amount": 9999 }),
    );
    assert_is_error(&result);

    // 10. list_windows：只读枚举
    let result = call(
        &mut client,
        10,
        "list_windows",
        json!({ "limit": 20, "only_visible": true }),
    );
    assert_not_error(&result);
    let wins = structured(&result);
    assert!(
        wins["windows"].as_array().is_some(),
        "应返回 windows 数组: {wins}"
    );
    assert!(wins["count"].as_u64().is_some());

    // 11. clipboard_get：只读；空剪贴板在部分平台会报错，只要进程不崩即可
    let result = call(&mut client, 11, "clipboard_get", json!({}));
    if result.pointer("/result/isError").and_then(Value::as_bool) != Some(true) {
        let clip = structured(&result);
        assert!(
            clip.get("text").and_then(Value::as_str).is_some(),
            "成功时应含 text 字段: {clip}"
        );
    }
}
