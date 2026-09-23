"""端点：平台任务服务（/api/platform/tasks，工具执行历史的统一存储）。"""

TASKS = "/api/platform/tasks"


def test_tasks_list_shape(client):
    r = client.get(TASKS, params={"tool_id": "bili2text", "limit": 10})
    assert r.status_code == 200
    data = r.json()
    assert isinstance(data["tasks"], list)
    for t in data["tasks"]:
        assert t["tool_id"] == "bili2text"
        assert t["status"] in {"succeeded", "failed"}
        assert "result_text" not in t  # 列表不含全文


def test_task_get_not_found(client):
    r = client.get(f"{TASKS}/00000000-0000-0000-0000-000000000000")
    assert r.status_code == 404
    assert r.json()["error"] == "task_not_found"


def test_extract_creates_task_record(client):
    """提取（无论成败）都会产生任务记录。"""
    before = client.get(TASKS, params={"tool_id": "bili2text"}).json()["tasks"]
    before_ids = {t["id"] for t in before}

    # 用一个确定解析成功的真实视频；成败都落记录
    client.post(
        "/api/tools/bili2text/extract",
        json={"input": "BV1J7hE6aEDQ"},
    )

    after = client.get(TASKS, params={"tool_id": "bili2text"}).json()["tasks"]
    assert len(after) == len(before) + 1
    new_ids = {t["id"] for t in after} - before_ids
    assert len(new_ids) == 1
    new_task = next(t for t in after if t["id"] in new_ids)
    assert new_task["input"] == "BV1J7hE6aEDQ"
    assert new_task["status"] in {"succeeded", "failed"}


def test_task_detail_and_delete(client):
    """列表拿最新一条 → 详情（含全文或错误）→ 删除 → 404。"""
    tasks = client.get(TASKS, params={"tool_id": "bili2text", "limit": 1}).json()["tasks"]
    assert tasks, "至少需要一条任务记录（先跑一次 extract）"
    task_id = tasks[0]["id"]

    r = client.get(f"{TASKS}/{task_id}")
    assert r.status_code == 200
    task = r.json()["task"]
    assert task["id"] == task_id
    if task["status"] == "succeeded":
        assert task["result_text"]

    r = client.delete(f"{TASKS}/{task_id}")
    assert r.status_code == 200
    assert client.get(f"{TASKS}/{task_id}").status_code == 404
