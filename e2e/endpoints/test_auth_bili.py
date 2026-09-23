"""端点：B 站扫码登录（/api/auth/bili/*，与 server/src/lib.rs auth 区一一对应）。

依赖真实 B 站接口（无需登录态的部分）。
"""

import re


def test_qrcode_generate(client):
    r = client.post("/api/auth/bili/qrcode")
    assert r.status_code == 200

    data = r.json()
    assert re.fullmatch(r"[0-9a-f]{32}", data["qrcode_key"])
    assert data["qr_content"].startswith("https://")
    assert data["expires_in_secs"] == 180


def test_qrcode_poll_requires_key(client):
    r = client.get("/api/auth/bili/qrcode/poll")
    assert r.status_code == 400
    assert r.json()["error"] == "missing_qrcode_key"


def test_qrcode_poll_fake_key_reports_expired(client):
    """伪造 key 真实轮询 B 站 → 应返回失效码 86038。"""
    r = client.get(
        "/api/auth/bili/qrcode/poll",
        params={"qrcode_key": "0" * 32},
    )
    assert r.status_code == 200
    data = r.json()
    assert data["code"] == 86038
    assert data["status"] == "expired"
    assert data["logged_in"] is False


def test_status_shape(client):
    r = client.get("/api/auth/bili/status")
    assert r.status_code == 200
    data = r.json()
    assert isinstance(data["has_credential"], bool)
    assert isinstance(data["logged_in"], bool)
