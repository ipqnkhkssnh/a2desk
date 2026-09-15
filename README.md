# a2desk

跨平台桌面控制 **MCP 服务器**（Rust 实现）。把「看屏幕 + 动鼠标 + 敲键盘 + 查应用」这套能力
暴露成标准 MCP 工具，任何支持 MCP 的客户端（Claude Desktop、Cursor、Cherry Studio、
自研 Agent……）都能直接调用，用来做 GUI 自动化、无人值守操作、桌面巡检等。

支持 **Windows / macOS / Linux(X11)**。

---

## 一、工具一览

### 屏幕 / 输入 / 应用

| 工具 | 说明 | 关键参数 |
| --- | --- | --- |
| `list_screens` | 获取所有屏幕的分辨率/位置/缩放/主屏标记 | — |
| `screenshot` | 截屏，返回 **base64 图片**（默认 jpeg，可 png） | `screen`、`region{x,y,width,height}`（可选）、`scale`、`max_width`、`format`、`quality` |
| `mouse_move` | 移动鼠标到指定屏幕的指定位置 | `screen`、`x`、`y`、`duration_ms`(平滑移动) |
| `mouse_click` | 鼠标点击 | `screen`、`x`、`y`、`button`(left/middle/right)、`count`(1=单击/2=双击) |
| `mouse_double_click` | 鼠标双击（固定左键） | `screen`、`x`、`y` |
| `mouse_drag` | 拖拽：按下 → 移动 → 松开，支持跨屏 | `from_screen/from_x/from_y`、`to_screen/to_x/to_y`、`button`(left/right)、`duration_ms` |
| `mouse_scroll` | 滚轮滚动 | `screen`、`x`、`y`(可选，先移过去)、`direction`(up/down/left/right)、`amount`(**1–100**) |
| `mouse_position` | 当前鼠标位置及其所在屏幕 | — |
| `keyboard_type` | 输入文本（支持 Unicode） | `text`、`interval_ms` |
| `keyboard_press` | 按键 / 组合键（按下并松开） | `keys`（`["Enter"]`、`["ctrl+shift+s"]`）、`repeat`、`interval_ms` |
| `keyboard_key_down` | 按下不松开（长按/组合键） | `keys` |
| `keyboard_key_up` | 松开按键，`["all"]` 松开全部 | `keys` |
| `list_apps` | 正在运行的程序（pid/名称/路径/命令行/CPU/内存/窗口） | `filter`、`pids`、`include_windows`、`only_with_windows`、`sort_by`、`limit` |
| `list_windows` | 扁平窗口列表（含可见区域、所在屏幕） | `filter`、`pid`、`screen_index`、`only_visible`、`sort_by`、`limit` |
| `focus_window` | 激活/前置窗口 | `id` / `title` / `pid` / `app_name` / `focused` |
| `move_window` | 移动窗口（**虚拟桌面绝对坐标**） | 选择器 + `x`、`y` |
| `resize_window` | 调整窗口大小 | 选择器 + `width`、`height` |
| `set_window_bounds` | 同时设置位置与尺寸 | 选择器 + `x`、`y`、`width`、`height` |
| `set_window_screen` | 把窗口放到指定屏幕 | 选择器 + `screen`、`margin` |
| `minimize_window` / `maximize_window` / `restore_window` / `close_window` | 窗口显隐与关闭 | 选择器 |
| `screenshot_window` | 按窗口截图 | 选择器 + `scale`/`max_width`/`format`/`quality` |
| `wait_for_window` | 等待匹配窗口出现 | 选择器 + `timeout_ms`、`poll_ms` |
| `find_text` | 无障碍树查找文本（返回屏幕坐标） | `query`、可选窗口选择器、`limit` |
| `click_text` | 查找文本并点击其中心 | `query`、可选窗口选择器、`button`、`count` |
| `wait_for_text` | 等待文本出现 | `query`、`timeout_ms`、`poll_ms` |
| `type_in_window` | 聚焦窗口 → 点中心 → 输入文本 | 选择器 + `text`、`interval_ms` |
| `clipboard_get` / `clipboard_set` | 读写剪贴板文本 | `text`（set） |

窗口类工具的**选择器**字段（可组合）：`id`、`title`（子串）、`pid`、`app_name`（子串）、`focused`。

`find_text` / `click_text`：Windows 走 UI Automation；macOS 走辅助功能（需授权）；Linux(X11) 窗口控制已支持，文本查找为扩展点（可先用截屏+点击）。

### 窗口编排

窗口选择器字段（多数窗口工具通用，可组合）：`id` / `title` / `pid` / `app_name` / `focused`。

| 工具 | 说明 |
| --- | --- |
| `list_windows` | 扁平列出顶层窗口（含 `visible_bounds`、所在屏幕） |
| `focus_window` | 激活到前台 |
| `move_window` | 移动到虚拟桌面绝对坐标 `(x,y)` |
| `resize_window` | 调整宽高 |
| `set_window_bounds` | 同时设置位置与尺寸 |
| `set_window_screen` | 把窗口放到指定屏幕（`screen` + 可选 `margin`） |
| `minimize_window` / `maximize_window` / `restore_window` / `close_window` | 显隐与关闭 |
| `screenshot_window` | 按窗口截图 |
| `wait_for_window` | 等待匹配窗口出现 |
| `type_in_window` | 聚焦 → 点窗口中心 → 输入文本 |

### 文本查找 / 剪贴板

| 工具 | 说明 |
| --- | --- |
| `find_text` | 无障碍树查找文本，返回屏幕坐标（Windows=UIA，macOS=AX/System Events，Linux=AT-SPI 扩展中） |
| `click_text` | 查找并点击文本中心 |
| `wait_for_text` | 等待文本出现 |
| `clipboard_get` / `clipboard_set` | 读写系统剪贴板文本 |

所有截图/鼠标工具都支持 `screen` 参数，取值可以是：

* 屏幕索引：`"0"`、`"1"`（来自 `list_screens` 的 `index`，**0 = 主屏**）
* 屏幕名称关键字：`"DELL"`、`"Built-in"`
* `"primary"` / `"main"` / `"主屏"`
* 省略 → 使用主屏

---

## 二、坐标系（重要）

整套工具只用**一套坐标**，避免"图上坐标"和"鼠标坐标"对不上：

```
屏幕信息(list_screens)   ── x/y/width/height：该屏幕在虚拟桌面中的位置与尺寸
鼠标工具 / region        ── 屏幕内局部坐标：相对该屏幕左上角的 (0,0)
截屏图片                 ── 默认 scale=1.0 时：1 图片像素 = 1 屏幕坐标单位
```

于是推荐的工作流是：

1. `list_screens` → 拿到屏幕索引和尺寸；
2. `screenshot`（默认 `scale=1.0`）→ 直接在图上量像素坐标；
3. 把量到的 `(像素X, 像素Y)` 原样填进 `mouse_click` 的 `x`、`y`，**不需要任何换算**。

如果显式传了 `max_width` 或 `scale != 1.0`，返回里的 `pixel_ratio` 会变化，此时：

```
屏幕坐标X = region.x + 图片X / pixel_ratio
屏幕坐标Y = region.y + 图片Y / pixel_ratio
```

> 说明：macOS 上坐标单位是**逻辑点(pt)**（Retina 屏 1 点 = 2 像素），Windows/Linux 上是**物理像素**。
> 截屏默认按 `scale=1.0` 缩放到与坐标单位 1:1；想保留 Retina 原始精细度可传 `scale`（最大 8）或 `format: "png"`。

坐标超出屏幕范围不会报错，会被**裁剪到屏幕边缘**，并在返回 JSON 里给出 `"clamped": true` 和提示。

---

## 三、编译

```bash
cd a2desk
cargo build --release
# 产物：target/release/a2desk
```

### 各平台前置依赖

<details open>
<summary><b>macOS</b></summary>

需要在「系统设置 → 隐私与安全性」里授权给**运行 a2desk 的程序**（终端 / MCP 客户端本体）：

* **屏幕录制**：`screenshot`、`list_apps` 的窗口标题需要
* **辅助功能**：所有鼠标/键盘工具需要

授权后必须**重启该程序**。默认情况下缺权限时会弹一次系统授权提示，可用 `--no-permission-prompt` 关闭。
</details>

<details>
<summary><b>Windows</b></summary>

无需额外安装。程序启动时会调用 `SetProcessDpiAwareness`，统一使用物理像素坐标；
多显示器下鼠标定位使用 `SetCursorPos`（`enigo` 自带的绝对定位只覆盖主屏）。
</details>

<details>
<summary><b>Linux</b></summary>

* 编译需要系统开发库：`libxcb`、`libxrandr`、`libwayland`（xcap 的 Wayland 支持）等，例如 Debian/Ubuntu：
  `sudo apt install libxcb1-dev libxrandr-dev libwayland-dev libpipewire-0.3-dev pkg-config`
* 运行需要 **X11**（`DISPLAY` 可用）。Wayland 下截屏依赖 xdg-desktop-portal、输入模拟依赖 `uinput`/libei，
  不同发行版差异较大，属于**尽力而为**支持。
</details>

---

## 四、接入 MCP 客户端

stdio 传输，客户端只需要启动这个进程即可（示例见 [`mcp.example.json`](mcp.example.json)）：

```json
{
  "mcpServers": {
    "a2desk": {
      "command": "/绝对路径/a2desk/target/release/a2desk",
      "args": []
    }
  }
}
```

* Claude Desktop：写进 `claude_desktop_config.json` 的 `mcpServers`
* Cursor / 其它客户端：用各自的 MCP servers 配置项，格式相同
* **日志全部写到 stderr**，stdout 只用于 MCP 协议，不会污染通信

命令行选项：

```
a2desk                      以 MCP stdio 服务器运行
a2desk --selftest           自检（屏幕/截屏/输入/应用）
a2desk --verbose            调试日志
a2desk --log-level debug    指定日志级别
a2desk --no-permission-prompt
```

---

## 五、自检

接入客户端前，先确认本机权限是否齐全：

```bash
./target/release/a2desk --selftest
```

还有一个**无副作用**的输入冒烟测试（会移动鼠标并精确还原，只按一下 Shift，
不点击、不输入文本、不滚动），用来确认鼠标键盘权限真的生效：

```bash
cargo build --release
python3 scripts/input-smoke.py
```

它会真的启动 a2desk 并以 MCP stdio 协议调用 `mouse_position` / `mouse_move` /
`keyboard_key_down` / `keyboard_key_up`，逐项打印结果：

```
① 当前位置： {"global_x": 1024, "global_y": 512, "screen_index": 0, "local_x": 1024, "local_y": 512}
② mouse_move -> (1094,572) 耗时 0.30s
③ 移动后位置： {"local_x": 1094, "local_y": 572, ...}
   -> 实际移动： ✅ 成功
④ 还原到原位置： ✅ 成功
⑤ keyboard_key_down [shift]： {...}
⑥ keyboard_key_up [all]： {"still_pressed": []}
⑦ keyboard_press [shift]： {...}
结果： 冒烟测试通过 ✅
```

> macOS 上如果 `--selftest` 的「输入控制」显示 `NoPermission`，说明当前进程没有
> **辅助功能**权限。TCC 会在进程内缓存判定结果，所以**授权后必须重启该进程**
> （终端 / MCP 客户端本体），仅关掉窗口不算。

`--selftest` 输出示例（macOS，权限齐全）：

```
== 屏幕 ==
  [0] P2768 1920x1080 @(0,0) scale=2 primary=true builtin=false
== 截屏 ==
  成功：480x270 pixel_ratio=0.250 19587 字节 jpeg
  区域截图成功：80x40 1393 字节
== 输入控制 ==
  鼠标当前位置：(1024, 512)
== 应用 ==
  进程总数（匹配）：725，返回 5（窗口信息可用：true）
  pid=5137 RustDesk cpu=23.9% mem=320MB windows=0
  ...
self-test 通过 ✅
```

---

## 六、实现说明

| 能力 | 依赖 |
| --- | --- |
| MCP 协议 | [`rmcp`](https://crates.io/crates/rmcp) 3.x（官方 Rust SDK，stdio 传输） |
| 屏幕枚举 / 截屏 / 窗口枚举 | [`xcap`](https://crates.io/crates/xcap) |
| 鼠标键盘模拟 | [`enigo`](https://crates.io/crates/enigo) |
| 图片编码 | `image`（jpeg / png） |
| 进程枚举 | `sysinfo` |

设计要点：

* **输入串行化**：`enigo::Enigo` 不是 `Sync` 且输入事件必须串行（按下→移动→松开），
  因此用一个专属线程持有 `Enigo`，异步侧通过 channel 下发命令；
  拖拽中途出错也会强制松开按键，避免鼠标卡在按下状态。
* **`Send` 安全**：`xcap::Monitor` 在 Windows 上含裸指针（非 `Send`），
  所有 async 工具函数里的坐标解析结果都只保留纯数据，绝不跨 `await` 持有 `Monitor`。
* **平台按键隔离**：`enigo::Key` 里大量变体（`A`、`Num0`、`Insert`、`BrightnessUp`…）
  是按平台 `#[cfg]` 门控的，因此字母/数字统一映射为 `Key::Unicode`，
  平台专属键分别放在 `non_mac_key` / `windows_key` / `unix_key` / `mac_only_key` 中。
* **"先移动再点击"必须等位置同步**（实测踩到的坑）：`enigo` 在 macOS 上实现
  `button()` / `key()` 时会**自己再调用一次 `location()`** 来决定事件投递坐标。
  如果移动之后立刻点击，这个 `location()` 往往还是旧值，于是点击被投递到旧坐标，
  **并且把光标"拉回"旧位置**——表现出来就是"`mouse_click` 带了 x/y 却完全没反应"。
  因此 `Worker::move_and_settle()` 会在移动后轮询 `location()`，直到它与目标坐标一致
  （最长 400ms）再发点击；拖拽的"按下前 / 松开前"也都做了同样的同步。
  这正是那种只有真机端到端测试才能发现的 bug。

### 平台验证状态

| 平台 | 验证方式 | 状态 |
| --- | --- | --- |
| macOS (aarch64) | 完整构建 + 单元测试 + MCP stdio 端到端测试 + **鼠标键盘全工具真机实测**（截屏定位、点击、双击选词、拖拽选文本、滚动结果列表、输入文本、组合键，并用截图 md5 逐项核对） | ✅ 实测通过 |
| Windows (x86_64) | `cargo check --target x86_64-pc-windows-msvc` | ✅ 编译通过（多屏 `SetCursorPos` 定位未实机验证） |
| Linux (x86_64) | 交叉编译受限于本机无 Linux 系统库；按键映射模块单独编译通过 | ⚠️ 未完整编译验证 |

---

## 七、开发

```bash
cargo test          # 单元测试 + 真实 MCP stdio 往返测试
cargo test --test mcp_stdio -- --nocapture   # 会启动真实进程跑一遍协议，并落一张截图到 target/
cargo clippy --all-targets
```

`tests/mcp_stdio.rs` 会真的 `spawn` 编译出的二进制，走
`initialize → notifications/initialized → tools/list → tools/call`，
校验工具齐全、截图是合法 JPEG、越界 region 被裁剪、非法参数返回工具级错误。

## License

MIT
