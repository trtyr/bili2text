"""端点：本地 ASR 转写（/api/tools/bili2text/transcribe，长任务）。

完整链路（下载 + 推理）耗时以分钟计，标 live；常规套件只测快速失败路径。
"""

import pytest

TRANSCRIBE = "/api/tools/bili2text/transcribe"


def test_transcribe_rejects_bad_input(client):
    r = client.post(TRANSCRIBE, json={"input": "https://example.com/nope"})
    assert r.status_code == 400
    assert r.json()["error"] == "bad_input"


@pytest.mark.live
def test_transcribe_full_pipeline(client):
    """完整转写（需模型已下载）：提交 → 轮询至终态 → 校验结果。"""
    r = client.post(TRANSCRIBE, json={"input": "BV1rKhq6QEU6"}, timeout=30)
    assert r.status_code == 200
    data = r.json()
    assert data["status"] == "running"
    task_id = data["task_id"]

    # 轮询最长 10 分钟
    import time

    deadline = time.time() + 600
    task = None
    while time.time() < deadline:
        task = client.get(f"/api/platform/tasks/{task_id}").json()["task"]
        if task["status"] in {"succeeded", "failed"}:
            break
        time.sleep(5)

    assert task is not None
    assert task["status"] == "succeeded", task.get("error")
    assert task["result_text"]
    assert task["result_srt"]
    assert (task["lines_count"] or 0) > 0
