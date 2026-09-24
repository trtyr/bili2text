"""扫码登录：终端二维码渲染冒烟（真实 B 站接口，无需人工扫码）。

完整扫码链路（手机确认）无法自动化，属人工验证项：跑 `bili2text login`
亲自扫一次即同时验证渲染正确性 + 轮询 + 登录态落盘。
"""

import os
import queue
import subprocess
import threading
import time
from pathlib import Path

from conftest import BIN


def test_login_renders_terminal_qrcode(tmp_path: Path):
    """login 应在 25s 内于终端渲染出块字符二维码，且可被 Ctrl-C/信号中断。"""
    home = tmp_path / "home"
    work = tmp_path / "work"
    home.mkdir()
    work.mkdir()
    env = {**os.environ, "HOME": str(home)}

    proc = subprocess.Popen(
        [str(BIN), "login"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        cwd=str(work),
        env=env,
    )
    lines: "queue.Queue[str]" = queue.Queue()

    def reader() -> None:
        assert proc.stdout is not None
        for line in proc.stdout:
            lines.put(line)

    thread = threading.Thread(target=reader, daemon=True)
    thread.start()

    deadline = time.time() + 25
    qr_seen = False
    while time.time() < deadline:
        if proc.poll() is not None:
            break  # 提前退出 = 异常
        try:
            line = lines.get(timeout=0.5)
        except queue.Empty:
            continue
        if "█" in line:  # 半块字符渲染的暗模块
            qr_seen = True
            break

    proc.terminate()
    proc.wait(timeout=10)
    assert qr_seen, "25s 内未渲染出终端二维码（检查 qrencode 渲染与 B 站出码接口）"
