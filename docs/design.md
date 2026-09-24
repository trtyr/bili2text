# 设计与实现

面向开发者 / 贡献者的深度文档。产品视角的内容（是什么、怎么用）见 [README](../README.md)。

## 调研结论（2026-09-23，实测有效）

### 字幕提取链路

- 接口：`GET api.bilibili.com/x/player/wbi/v2`（aid/bvid + cid，需 wbi 签名）
- 返回 `data.subtitle.subtitles[]`：CC 字幕与 AI 字幕混排，`ai_type` / `ai_status` 区分，AI 字幕 lan 形如 `ai-zh`；多语言可并存（zh-Hans / en-US…）
- 字幕本体：`aisubtitle.hdslb.com` 返回 JSON，`body[{from, to, content}]`，可转纯文本 / SRT
- **未登录时 subtitles 为空数组**——扫码登录是刚需
- 来源：bilibili-API-collect `docs/video/player.md`（pskdje fork，2026-01 同步）

### 扫码登录协议

- `GET passport.bilibili.com/x/passport-login/web/qrcode/generate` → `{qrcode_key, url}`
- 轮询 `.../web/qrcode/poll?qrcode_key=`：86101 未扫 / 86090 已扫未确认 / 86038 失效 / 0 成功；成功时 Set-Cookie 下发 SESSDATA
- 扫码登录无验证码环节

### ASR 选型（已定案 SenseVoice + sherpa-rs）

- **SenseVoice Small**（阿里 FunAudioLLM）：中 / 英 / 日 / 韩 / 粤五语，中文显著优于 Whisper；int8 约 230MB（仓库内含全部资产约 1.1GB）；CPU 实时约 15-20 倍速
- **sherpa-rs**（sherpa-onnx 官方 Rust 绑定）：纯 Rust 进程内推理，无 Python 运行时；支持 silero VAD + SenseVoice 组合（分段带时间戳，可生成 SRT）
- 否决的备选：SenseVoice GGUF / llama.cpp 运行时；faster-whisper（需 Python sidecar）

### 视频下载

- yt-dlp 外部进程（B 站风控跟进最及时；`--cookies` 喂 Netscape 格式 SESSDATA cookie；`-f bestaudio` + ffmpeg 转 16k mono wav）
- 后期可换自实现 playurl（fnval=16 DASH）

## 架构

```text
crates/
├── bili2text/       # CLI 应用本体：clap 子命令 + 提取/转写编排 + 历史存储 + Markdown 输出
├── bili-client/     # B 站客户端：扫码登录 / wbi 签名（含官方测试向量单测）/ view / 字幕 / b23.tv 解析
├── downloader/      # yt-dlp + ffmpeg 外部进程封装（16k mono wav）
└── asr/             # SenseVoice int8（sherpa-rs 进程内）+ silero VAD + WAV 读取
```

数据流：

```text
输入 URL ──解析 BV──▶ video_view ──▶ subtitle_tracks ──有轨──▶ pick_track ──▶ 字幕 JSON ──▶ Doc ──▶ *.md / *.srt
                                        │无轨（或 --transcribe）
                                        ▼
                    credential ──▶ yt-dlp 下载音轨 ──▶ ffmpeg 16k mono ──▶ SenseVoice + VAD ──▶ Doc
```

历史（SQLite）两条路径都写：短任务完成后落一条；长任务先写 running，
阶段心跳（downloading / transcribing），终态回写结果。

## 数据目录

`~/.local/share/bili2text/`：

| 路径 | 内容 |
| --- | --- |
| `credential.json` | B 站登录态（SESSDATA 等 cookie 集合） |
| `tasks.db` | 提取 / 转写历史（SQLite；`tool_id` 列为历史遗留，恒为 `bili2text`） |
| `models/sherpa-onnx-sense-voice-…` | SenseVoice int8 模型（`scripts/download-model.sh` 下载） |
| `logs/bili2text.log` | 运行日志（追加式：argv / cwd / 步骤 / 耗时 / 错误全文） |
| `audio/` | 转写用临时音频（用完即删） |
| `bili-cookies.txt` | 转写时导出的 Netscape cookie（yt-dlp 用） |

## 错误处理与退出码设计

`AppError` 枚举 13 个变体，每个变体对应独立退出码：

| 码 | 变体 | 透传内容 |
| --- | --- | --- |
| 1 | `Other` | 其他未分类 |
| 2 | `Usage` | 用法 / 参数问题（含可用语言列表等上下文） |
| 10 | `BadInput` | 原始输入文本 |
| 20 | `BiliApi` | B 站接口 code + message 原样 |
| 21 | `BiliNetwork` | reqwest 错误原文 |
| 22 | `BiliData` | JSON 解析错误原文 |
| 30 | `NotLoggedIn` | — |
| 31 | `NoSubtitle` | bvid + 标题 |
| 40 | `ExternalMissing` | 缺失的二进制名（yt-dlp / ffmpeg） |
| 41 | `YtDlpFailed` | yt-dlp 进程 stderr 原样 |
| 42 | `FfmpegFailed` | ffmpeg 进程 stderr 原样 |
| 50 | `Asr` | AsrError 原文 |
| 60 | `Storage` | 本地 IO / SQLite 错误 |

设计原则：**底层工具的错误逐变体 match 后原文透传**，不做二次概括——
`from_bili` / `from_download` 两个映射函数有单测锁定行为。main 统一输出
`错误[码]：详情` 并以对应码退出；错误同时写入日志文件。

## 日志

- 位置：`~/.local/share/bili2text/logs/bili2text.log`，追加式
- 级别：`INFO`（运行参数 / 耗时，仅文件）、`STEP`（进度，stderr + 文件）、`WARN`（非致命，stderr + 文件）、`ERROR`（仅文件，终端由 main 统一输出）
- stderr 只保留人类可读进度；stdout 输出结果摘要与文档路径（可管道）

## 测试

- **单元测试**（Rust，19 个）：wbi 签名官方测试向量、BV 解析、SRT 格式、
  字幕挑选策略、退出码互异、底层错误映射、文档写盘
- **端到端**（Python pytest + subprocess 驱动真实二进制，18 个）：
  - 数据隔离：`HOME` 重定向到临时目录，不碰真实登录态 / 历史 / 日志
  - `cli_with_auth`：把真实 credential 拷进隔离 HOME（字幕接口未登录时列表为空，提取用例实际依赖登录态）
  - 覆盖：退出码分类、日志落盘、真实提取（md / srt / 历史联动）、
    终端二维码渲染冒烟、完整转写（`live` 标记）

```bash
cd e2e && ./.venv/bin/python -m pytest -v               # 全量
./.venv/bin/python -m pytest -m "not live"               # 快速回归
```

## 演进

- **2026-09-23** 以「工具集合平台」立项（Rust axum + React 门户 + pytest e2e），
  当日完成字幕提取与 ASR 两条链路
- **2026-09-24** 去平台化（删 Tool trait / 工具目录），同日进一步去掉 Web 层
  （前端 + HTTP 服务），收敛为纯 CLI；错误 / 日志体系按 review 重做；
  e2e 以 Python subprocess 方案重建

## 风险与限制（设计侧）

- B 站风控（cookie / UA / 频控）：登录态管理保持干净，下载侧由 yt-dlp 分担
- AI 字幕非全覆盖：ASR 是必要兜底，不是附加功能
- 推理 RTF 约 0.7-1×（M 系 CPU），可分段并行优化（未做）
