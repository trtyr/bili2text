"""e2e 公共设施：subprocess 驱动真实 bili2text 二进制。

数据隔离：cli fixture 把 HOME 重定向到临时目录——登录态、历史库、日志
全部落在临时目录，不碰真实数据。BILI2TEXT_BIN 可指定被测二进制，
缺省 target/debug/bili2text（先 cargo build）。
"""

import os
import shutil
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
BIN = Path(os.environ.get("BILI2TEXT_BIN", REPO_ROOT / "target" / "debug" / "bili2text"))

TEST_BVID = "BV1GJ411x7h7"  # 经典 MV，长期稳定存在，有官方中文字幕


def _require_bin() -> None:
    if not BIN.exists():
        pytest.exit(f"被测二进制不存在：{BIN}（先在仓库根执行 cargo build）", returncode=1)


def _runner(home: Path, cwd: Path):
    env = {**os.environ, "HOME": str(home)}

    def run(*args: str, timeout: float = 60) -> subprocess.CompletedProcess:
        return subprocess.run(
            [str(BIN), *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(cwd),
            env=env,
        )

    return run


@pytest.fixture
def cli(tmp_path: Path) -> SimpleNamespace:
    """隔离数据目录（HOME → tmp）的 CLI 运行器。"""
    _require_bin()
    home = tmp_path / "home"
    work = tmp_path / "work"
    home.mkdir()
    work.mkdir()
    return SimpleNamespace(run=_runner(home, work), home=home, work=work)


def has_real_credential() -> bool:
    """真实 HOME 下是否已有登录态（live 用例前置条件）。"""
    cred = Path.home() / ".local" / "share" / "bili2text" / "credential.json"
    return cred.exists() and cred.stat().st_size > 10


@pytest.fixture
def cli_with_auth(tmp_path: Path) -> SimpleNamespace:
    """隔离 HOME + 预置真实登录态。

    B 站字幕接口未登录时列表为空，所以提取类用例其实依赖登录态：
    这里把真实 credential.json 拷进隔离 HOME（历史 / 日志仍是干净的
    临时目录），无登录态则 skip。
    """
    _require_bin()
    src = Path.home() / ".local" / "share" / "bili2text" / "credential.json"
    if not (src.exists() and src.stat().st_size > 10):
        pytest.skip("需要真实登录态（先 bili2text login）")
    home = tmp_path / "home"
    work = tmp_path / "work"
    (home / ".local" / "share" / "bili2text").mkdir(parents=True)
    work.mkdir()
    shutil.copy(src, home / ".local" / "share" / "bili2text" / "credential.json")
    return SimpleNamespace(run=_runner(home, work), home=home, work=work)


@pytest.fixture
def real_cli(tmp_path: Path) -> SimpleNamespace:
    """真实 HOME 的 CLI 运行器（真实登录态 / 真实模型），仅 live 用例使用。"""
    _require_bin()
    work = tmp_path / "work"
    work.mkdir()
    env = {**os.environ}

    def run(*args: str, timeout: float = 60) -> subprocess.CompletedProcess:
        return subprocess.run(
            [str(BIN), *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(work),
            env=env,
        )

    return SimpleNamespace(run=run, work=work)
