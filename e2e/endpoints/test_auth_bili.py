"""端点：B 站扫码登录（工具内部功能，挂 /api/tools/bili2text/auth/*）。

依赖真实 B 站接口（无需登录态的部分）。
"""

import re

AUTH = "/api/tools/bili2text/auth"


def test_qrcode_generate(client):
    r = client.post(f"{AUTH}/qrcode")
    assert r.status_code == 200

    data = r.json()
    assert re.fullmatch(r"[0-9a-f]{32}", data["qrcode_key"])
    assert data["qr_content"].startswith("https://")
    assert data["expires_in_secs"] == 180


def test_qrcode_poll_requires_key(client):
    r = client.get(f"{AUTH}/qrcode/poll")
    assert r.status_code == 400
    assert r.json()["error"] == "missing_qrcode_key"


def test_qrcode_poll_fake_key_reports_expired(client):
    """伪造 key 真实轮询 B 站 → 应返回失效码 86038。"""
    r = client.get(f"{AUTH}/qrcode/poll", params={"qrcode_key": "0" * 32})
    assert r.status_code == 200
    data = r.json()
    assert data["code"] == 86038
    assert data["status"] == "expired"
    assert data["logged_in"] is False


def test_status_shape(client):
    r = client.get(f"{AUTH}/status")
    assert r.status_code == 200
    data = r.json()
    assert isinstance(data["has_credential"], bool)
    assert isinstance(data["logged_in"], bool)
