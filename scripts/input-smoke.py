#!/usr/bin/env python3
"""a2desk 输入冒烟测试：驱动真实的 a2desk MCP 服务（stdio）。

只做**无副作用**的验证：
  * 读取鼠标位置
  * 移动鼠标 (+dx,+dy) 并检测是否真的动了，然后精确还原到原位
  * 仅按下/松开 Shift（不输入文本、不点击、不滚动）

用法：
    cargo build --release
    python3 scripts/input-smoke.py
"""
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.environ.get("A2DESK_BIN") or os.path.join(ROOT, "target", "release", "a2desk")


class Client:
    def __init__(self):
        self.p = subprocess.Popen(
            [BIN, "--no-permission-prompt"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, bufsize=1,
        )
        self.id = 0

    def send(self, obj):
        self.p.stdin.write(json.dumps(obj) + "\n")
        self.p.stdin.flush()

    def request(self, method, params):
        self.id += 1
        rid = self.id
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("server closed stdout")
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == rid:
                return msg

    def call(self, tool, args=None):
        r = self.request("tools/call", {"name": tool, "arguments": args or {}})
        res = r.get("result", r.get("error"))
        if isinstance(res, dict) and res.get("isError"):
            text = res.get("content", [{}])[0].get("text", "")
            return {"__error__": text}
        sc = res.get("structuredContent")
        if sc:
            return sc
        return json.loads(res["content"][0]["text"])

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        self.p.wait(timeout=5)


def main():
    c = Client()
    ok = True
    init = c.request("initialize", {
        "protocolVersion": "2024-11-05", "capabilities": {},
        "clientInfo": {"name": "input-smoke", "version": "0"},
    })
    c.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    print("initialize ->", init["result"]["serverInfo"])

    pos0 = c.call("mouse_position")
    if "__error__" in pos0:
        print("FAIL 鼠标位置读取失败（输入权限仍未生效）：", pos0["__error__"])
        c.close()
        return 1
    print("① 当前位置：", json.dumps(pos0, ensure_ascii=False))
    if "local_x" not in pos0:
        print("FAIL 无法确定所在屏幕")
        c.close()
        return 1

    screen = pos0.get("screen_index", 0)
    lx, ly = pos0["local_x"], pos0["local_y"]

    # 选一个不会越界的位移，测试完再精确还原
    dx = 70
    dy = 60
    if lx + dx > 1900:
        dx = -70
    if ly + dy > 1020:
        dy = -60
    tx, ty = lx + dx, ly + dy

    t0 = time.time()
    r = c.call("mouse_move", {"screen": screen, "x": tx, "y": ty, "duration_ms": 300})
    if "__error__" in r:
        print("FAIL mouse_move:", r["__error__"])
        ok = False
    else:
        print(f"② mouse_move -> ({tx},{ty}) 耗时 {time.time()-t0:.2f}s", json.dumps(r, ensure_ascii=False))

    pos1 = c.call("mouse_position")
    print("③ 移动后位置：", json.dumps(pos1, ensure_ascii=False))
    moved = (pos1.get("local_x"), pos1.get("local_y")) == (tx, ty)
    print("   -> 实际移动：", "✅ 成功" if moved else "❌ 未生效")
    ok = ok and moved

    # 还原
    r = c.call("mouse_move", {"screen": screen, "x": lx, "y": ly, "duration_ms": 0})
    pos2 = c.call("mouse_position")
    restored = (pos2.get("local_x"), pos2.get("local_y")) == (lx, ly)
    print("④ 还原到原位置：", "✅ 成功" if restored else f"❌ 失败 {pos2}")
    ok = ok and restored

    # 键盘：只按 Shift（单独按 Shift 不产生任何字符/快捷键）
    r = c.call("keyboard_key_down", {"keys": ["shift"]})
    print("⑤ keyboard_key_down [shift]：", json.dumps(r, ensure_ascii=False))
    ok = ok and "__error__" not in r
    time.sleep(0.15)
    r = c.call("keyboard_key_up", {"keys": ["all"]})
    print("⑥ keyboard_key_up [all]：", json.dumps(r, ensure_ascii=False))
    ok = ok and "__error__" not in r

    # 键盘：组合键解析 + 真实发送（Shift 单击，仍然无副作用）
    r = c.call("keyboard_press", {"keys": ["shift"], "repeat": 1})
    print("⑦ keyboard_press [shift]：", json.dumps(r, ensure_ascii=False))
    ok = ok and "__error__" not in r

    c.close()
    print("\n结果：", "冒烟测试通过 ✅" if ok else "存在失败项 ❌")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
