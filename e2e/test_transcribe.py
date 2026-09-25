"""本地转写完整链路（live）：下载 → 转码 → SenseVoice 推理 → 落盘。

需要真实登录态与 yt-dlp/ffmpeg；完整跑一次约 2-3 分钟。
登录后手动跑：pytest -m live
"""

import shutil

import pytest

from conftest import TEST_BVID, has_real_credential

pytestmark = pytest.mark.live


@pytest.mark.skipif(not has_real_credential(), reason="需要真实登录态（先 bili2text login）")
@pytest.mark.skipif(shutil.which("yt-dlp") is None, reason="缺 yt-dlp")
@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="缺 ffmpeg")
def test_transcribe_full_pipeline(real_cli, transcribe_enabled):
    r = real_cli.run(TEST_BVID, "--transcribe", timeout=600)
    assert r.returncode == 0, f"stderr: {r.stderr}"

    # 进度阶段走了两步
    assert "[1/2] 下载音频" in r.stderr
    assert "[2/2] SenseVoice 本地转写中" in r.stderr

    # 产物：Markdown，文本来源标注本地转写
    mds = list(real_cli.work.glob("*.md"))
    assert len(mds) == 1
    content = mds[0].read_text(encoding="utf-8")
    assert "本地转写（SenseVoice" in content
    assert "## 正文" in content

    # 历史联动：asr_transcribe 成功记录可回看
    h = real_cli.run("history")
    assert h.returncode == 0
    assert "✓" in h.stdout
