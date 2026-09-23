"""端点：GET /api/health（与 server/src/lib.rs 中 health 一一对应）。"""


def test_health_returns_ok(client):
    r = client.get("/api/health")
    assert r.status_code == 200
    assert r.json()["status"] == "ok"


def test_health_content_type_is_json(client):
    r = client.get("/api/health")
    assert r.headers["content-type"].startswith("application/json")
