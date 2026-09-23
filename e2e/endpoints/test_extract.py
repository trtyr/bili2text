"""端点：bili2text 字幕提取（/api/tools/bili2text/tracks 与 /extract）。

依赖真实 B 站接口。未登录态下 AI 字幕不可见，tracks 为空 → extract 报 no_subtitle；
真实提取的完整链路用 live 标记（登录后手动跑：`pytest -m live`）。

测试视频：BV1GJ411x7h7（经典 MV，长期稳定存在）。
"""

import pytest

BVID_INPUT = "https://www.bilibili.com/video/BV1GJ411x7h7/"


def test_extract_rejects_bad_input(client):
    r = client.post("/api/tools/bili2text/extract", json={"input": "https://example.com/x"})
    assert r.status_code == 400
    assert r.json()["error"] == "bad_input"


def test_tracks_without_login_returns_empty(client):
    r = client.post("/api/tools/bili2text/tracks", json={"input": BVID_INPUT})
    assert r.status_code == 200
    data = r.json()
    assert data["bvid"] == "BV1GJ411x7h7"
    assert data["title"]
    assert isinstance(data["tracks"], list)


def test_extract_valid_video_returns_result_or_no_subtitle(client):
    """合法视频：登录时成功返回全文；未登录时报告 no_subtitle。两者都算链路正确。"""
    r = client.post("/api/tools/bili2text/extract", json={"input": BVID_INPUT})
    assert r.status_code == 200
    data = r.json()
    if data.get("error") == "no_subtitle":
        assert data["title"]
    else:
        assert "error" not in data
        assert data["text"]
        assert data["srt"]
        assert data["lines_count"] > 0


@pytest.mark.live
def test_extract_full_chain_with_login(client):
    """完整链路（需登录态）：登录后运行 pytest -m live 验证。"""
    r = client.post("/api/tools/bili2text/extract", json={"input": BVID_INPUT})
    assert r.status_code == 200
    data = r.json()
    assert "error" not in data
    assert data["text"]
    assert data["srt"]
    assert data["lines_count"] > 0
