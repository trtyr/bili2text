"""用法与参数错误：退出码分类（离线，不依赖网络）。

约定：1 其他 / 2 用法 / 10 输入解析（详见 README 退出码表）。
"""


def test_help_exits_0(cli):
    r = cli.run("--help")
    assert r.returncode == 0
    assert "bili2text" in r.stdout
    assert "--transcribe" in r.stdout  # 关键选项在用法里可见


def test_version_exits_0(cli):
    r = cli.run("--version")
    assert r.returncode == 0


def test_no_input_exits_2_usage(cli):
    r = cli.run()
    assert r.returncode == 2
    assert "缺少视频输入" in r.stderr


def test_bad_input_exits_10_with_original_text(cli):
    r = cli.run("https://example.com/nothing-here")
    assert r.returncode == 10
    # 原始输入原样透传，不被笼统化
    assert "https://example.com/nothing-here" in r.stderr


def test_history_show_without_id_exits_2(cli):
    r = cli.run("history", "show")
    assert r.returncode == 2
    assert "id" in r.stderr


def test_history_show_unknown_id_exits_2(cli):
    r = cli.run("history", "show", "deadbeef")
    assert r.returncode == 2
    assert "找不到记录" in r.stderr


def test_history_rm_unknown_id_exits_2(cli):
    r = cli.run("history", "rm", "deadbeef")
    assert r.returncode == 2


def test_history_empty_list_exits_0(cli):
    """隔离 HOME 下无任何历史：空列表是正常输出，不是错误。"""
    r = cli.run("history")
    assert r.returncode == 0
    assert "还没有历史记录" in r.stdout


def test_status_without_credential_exits_0(cli):
    """隔离 HOME 下未登录是正常状态查询结果，不是错误。"""
    r = cli.run("status")
    assert r.returncode == 0
    assert "未登录" in r.stdout
