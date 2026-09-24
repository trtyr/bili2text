"""日志：详细日志必须落在全局数据目录 logs/bili2text.log（离线可验）。"""


def test_log_file_created_under_data_dir(cli):
    r = cli.run("history")
    assert r.returncode == 0

    log = cli.home / ".local" / "share" / "bili2text" / "logs" / "bili2text.log"
    assert log.exists(), "运行后全局目录下应生成日志文件"

    content = log.read_text(encoding="utf-8")
    assert "[INFO] start argv=" in content  # 运行参数被记录


def test_log_records_argv_and_duration(cli):
    cli.run("history")
    log = cli.home / ".local" / "share" / "bili2text" / "logs" / "bili2text.log"
    content = log.read_text(encoding="utf-8")
    assert '"history"' in content  # 本次运行的子命令可见
    assert "[INFO] done in" in content  # 耗时被记录


def test_error_goes_to_log_with_exit_code(cli):
    """错误也要进日志，且带上退出码。"""
    r = cli.run("https://example.com/nothing-here")
    assert r.returncode == 10
    log = cli.home / ".local" / "share" / "bili2text" / "logs" / "bili2text.log"
    content = log.read_text(encoding="utf-8")
    assert "exit 10" in content
