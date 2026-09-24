"""字幕提取：真实 B 站接口（无需登录态），断言退出码 + 产物文件 + 历史联动。

测试视频：BV1GJ411x7h7（经典 MV，长期稳定存在，有官方中文字幕）。
依赖登录态的完整链路（AI 字幕等）归 live。
"""

from conftest import TEST_BVID


def test_extract_success_writes_md_and_history(cli_with_auth):
    r = cli_with_auth.run(TEST_BVID)
    assert r.returncode == 0, f"stderr: {r.stderr}"

    # stdout 摘要与文档路径
    assert "✓" in r.stdout
    assert "已保存：" in r.stdout

    # 产物：当前目录（work）下的 <标题>.md
    mds = list(cli_with_auth.work.glob("*.md"))
    assert len(mds) == 1, f"应恰好生成一个 md：{mds}"
    content = mds[0].read_text(encoding="utf-8")
    assert content.startswith("# ")  # 标题
    assert "文本来源" in content  # 信息表
    assert "## 正文" in content
    assert "| 来源 |" in content and TEST_BVID in content  # 来源行含 BV 号

    # 历史联动：出现一条成功记录
    h = cli_with_auth.run("history")
    assert h.returncode == 0
    assert "✓" in h.stdout
    assert "Never Gonna Give You Up" in h.stdout


def test_extract_srt_flag_exports_srt(cli_with_auth):
    r = cli_with_auth.run(TEST_BVID, "--srt")
    assert r.returncode == 0, f"stderr: {r.stderr}"

    srts = list(cli_with_auth.work.glob("*.srt"))
    assert len(srts) == 1
    srt = srts[0].read_text(encoding="utf-8")
    assert "-->" in srt  # SRT 时间轴
    assert "已保存：" in r.stdout
    assert r.stdout.count("已保存：") == 2  # md + srt 各提示一次


def test_extract_output_flag_writes_to_path(cli_with_auth):
    out = cli_with_auth.work / "custom" / "note.md"
    r = cli_with_auth.run(TEST_BVID, "-o", str(out))
    assert r.returncode == 0, f"stderr: {r.stderr}"
    assert out.exists(), "-o 指定路径（含子目录）应生效"


def test_extract_unknown_lang_exits_2_listing_available(cli_with_auth):
    """指定不存在的字幕语言 = 用法错误（2），并列出可用语言。"""
    r = cli_with_auth.run(TEST_BVID, "--lang", "xx-YY")
    assert r.returncode == 2
    assert "xx-YY" in r.stderr
    assert "可用语言" in r.stderr
