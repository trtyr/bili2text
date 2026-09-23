"""e2e 公共设施：目标服务地址与共享 HTTP 客户端。

目标地址通过环境变量 TOOLBOX_BASE_URL 覆盖，默认本机 8080。
"""

import os

import httpx
import pytest

BASE_URL = os.environ.get("TOOLBOX_BASE_URL", "http://127.0.0.1:8080")


@pytest.fixture(scope="session")
def client() -> httpx.Client:
    with httpx.Client(base_url=BASE_URL, timeout=10.0) as c:
        yield c
