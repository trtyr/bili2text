"""端点：GET /api/tools 与 GET /api/tools/{id}/info（工具目录 + 工具自述）。"""

TOOL_ID = "bili2text"


def test_tools_list_contains_bili2text(client):
    r = client.get("/api/tools")
    assert r.status_code == 200

    tools = r.json()
    assert isinstance(tools, list)
    ids = [t["id"] for t in tools]
    assert TOOL_ID in ids


def test_tools_list_entry_has_name_and_description(client):
    r = client.get("/api/tools")
    tools = r.json()
    entry = next(t for t in tools if t["id"] == TOOL_ID)
    assert entry["name"] == "B站视频转文字"
    assert entry["description"]


def test_tool_info_reports_modes(client):
    r = client.get(f"/api/tools/{TOOL_ID}/info")
    assert r.status_code == 200

    data = r.json()
    assert data["id"] == TOOL_ID
    assert "subtitle" in data["modes"]
    assert "asr" in data["modes"]


def test_unknown_tool_returns_404(client):
    r = client.get("/api/tools/no_such_tool/info")
    assert r.status_code == 404
