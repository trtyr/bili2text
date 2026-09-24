# bili2text

把 B 站视频变成文字的命令行工具：提取官方 / AI 字幕导出纯文本与 SRT；
无字幕视频用本地 SenseVoice 模型转写。纯 CLI、零常驻服务，登录态、历史、
模型全部留在本机。

## 用法

```bash
# 提取字幕（官方优先、AI 次之，自动选中文），存为 ./<标题>.md
bili2text https://www.bilibili.com/video/BV1GJ411x7h7/

# 同时导出 .srt 字幕文件
bili2text BV1GJ411x7h7 --srt

# 指定输出路径 / 字幕语言
bili2text BV1GJ411x7h7 -o ~/Documents/notes/rick.md
bili2text BV1GJ411x7h7 --lang ai-zh

# 无字幕视频：跳过字幕，本地 SenseVoice 转写（CPU，耗时较长）
bili2text BV1GJ411x7h7 --transcribe

# 账号（AI 字幕与高音质音频需要登录）
bili2text login     # 终端渲染二维码，B 站 App 扫码
bili2text status
bili2text logout

# 历史（SQLite，可回看全文）
bili2text history             # 列表
bili2text history show <id>   # 看全文（id 支持前缀）
bili2text history rm <id>
```

输入支持：BV 号、视频页链接、b23.tv 短链。

## 退出码

按错误类别区分，便于脚本判断：

| 码 | 含义 |
| --- | --- |
| 0 | 成功 |
| 1 | 其他未分类错误 |
| 2 | 用法 / 参数问题 |
| 10 | 无法解析 BV 号 |
| 20 | B 站接口返回业务错误（code/message 原样透传） |
| 21 | 网络请求失败 |
| 22 | B 站响应解析失败 |
| 30 | 未登录或登录态失效 |
| 31 | 视频没有可用字幕（提示加 `--transcribe`） |
| 40 | 外部依赖缺失（yt-dlp / ffmpeg） |
| 41 | yt-dlp 下载失败（stderr 原样透传） |
| 42 | ffmpeg 转码失败（stderr 原样透传） |
| 50 | 本地转写引擎错误 |
| 60 | 本地存储 / IO 错误 |

## 架构

```text
crates/
├── bili2text/       # CLI 应用本体（clap + 编排 + 历史存储 + 输出）
├── bili-client/     # B 站接口客户端：扫码登录 / wbi 签名 / 视频与字幕接口
├── downloader/      # 音频下载：yt-dlp + ffmpeg（16k mono wav）
└── asr/             # 本地转写：SenseVoice Small（sherpa-onnx）+ silero VAD
scripts/
└── download-model.sh  # 下载 ASR 模型（首次使用 --transcribe 前跑一次）
```

数据目录 `~/.local/share/bili2text/`：

| 路径 | 内容 |
| --- | --- |
| `credential.json` | B 站登录态（扫码后落盘） |
| `tasks.db` | 提取 / 转写历史（SQLite） |
| `models/` | SenseVoice 模型（约 1.1GB） |
| `logs/bili2text.log` | 运行日志（追加式，含参数 / 步骤 / 错误） |
| `audio/` | 转写用临时音频（用完即删） |

### 字幕链路（调研结论）

- `GET /x/player/wbi/v2` 返回 `data.subtitle.subtitles[]`
  （CC 与 AI 字幕混排，`ai_type`/`ai_status` 区分，多语言可选）；
  字幕本体为 `aisubtitle.hdslb.com` JSON（`body[{from,to,content}]`）。
  **未登录该列表为空**。
- 登录：扫码获取登录态 —— `qrcode/generate` 出码，轮询
  `qrcode/poll`（86101 未扫 / 86090 已扫待确认 / 86038 失效 / 0 成功），
  登录态（SESSDATA）落库。

### ASR 链路

无字幕（或不想要 AI 字幕）时，yt-dlp 拉音频轨 → ffmpeg 转 16k mono wav →
**SenseVoice Small**（sherpa-onnx，经 sherpa-rs 绑定，纯 Rust 进程内）+
silero VAD 分段转写，中文效果优于 Whisper，CPU 实时约 15-20 倍。

## 本地开发

```bash
cargo build                      # 构建
cargo test                       # 单元测试（Rust）
cargo run -- <BV号|链接>          # 开发运行
cargo install --path crates/bili2text   # 装进 PATH
```

外部依赖：`yt-dlp`、`ffmpeg`（转写链路需要；可用 `YTDLP_BIN` / `FFMPEG_BIN`
环境变量指定路径）。

### 端到端测试（Python）

```bash
cd e2e && python3 -m venv .venv && ./.venv/bin/pip install -r requirements.txt
cargo build && cd e2e && ./.venv/bin/python -m pytest -v          # 全量
./.venv/bin/python -m pytest -m "not live"                        # 跳过需登录态的慢用例
./.venv/bin/python -m pytest -m live -v                           # 仅 live（转写全链路）
```

- 用 `BILI2TEXT_BIN` 指定被测二进制，缺省 `target/debug/bili2text`
- 数据隔离：测试把 `HOME` 重定向到临时目录，不碰真实登录态 / 历史 / 日志
- `cli_with_auth` fixture 会把真实登录态拷进隔离 HOME（字幕接口必须登录），
  无登录态时自动 skip
- `live` 标记 = 真实登录态 + 完整转写（约 2-3 分钟）；login 的完整扫码
  属人工验证项（自动化只覆盖二维码渲染冒烟）

## 演进

- 2026-09-23 以「工具集合平台」立项（Rust axum + React 门户 + pytest e2e）
- 2026-09-24 去平台化收敛为单一工具；同日进一步去掉 Web 层（前端 + HTTP
  服务），改为纯 CLI。功能无损：提取 / 转写 / 历史全保留。
- 2026-09-24 e2e 以 Python（pytest + subprocess 驱动真实二进制）重建，
  数据隔离 + 退出码断言 + 真实链路 live 用例。
