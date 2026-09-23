# Toolbox

个人小工具集合平台。第一个工具：**B 站视频转文字（bili2text）**。

## 架构

```text
crates/
├── server/          # axum HTTP 服务：平台端点 + 工具路由挂载 + 静态托管（规划中）
├── platform-core/   # 平台核心：Tool trait / 工具目录 / 通用任务模型
├── bili-client/     # B 站接口客户端：扫码登录 / wbi 签名 / 字幕（接入中）
└── tool-bili2text/  # B 站视频转文字工具（字幕提取 + 本地 ASR 转写）
frontend/            # React + Vite + TS 工具门户
e2e/                 # Python 端到端测试（pytest + httpx，与 API 端点 1:1 对应）
```

### 平台化

新工具 = 实现一次 `platform_core::Tool`（id / 名称 / 描述 / 自有路由），
在 `server::registry()` 加一行，门户与任务体系自动识别。

### bili2text 设计（调研结论）

- **字幕链路**：`GET /x/player/wbi/v2` 返回 `data.subtitle.subtitles[]`
  （CC 与 AI 字幕混排，`ai_type`/`ai_status` 区分，多语言可选）；
  字幕本体为 `aisubtitle.hdslb.com` JSON（`body[{from,to,content}]`）。
  **未登录该列表为空**。
- **登录**：扫码获取登录态 —— `qrcode/generate` 出码，前端渲染，轮询
  `qrcode/poll`（86101 未扫 / 86090 已扫待确认 / 86038 失效 / 0 成功），
  登录态（SESSDATA）落库。
- **ASR 链路**：无字幕（或不想要 AI 字幕）时，yt-dlp 拉音频轨 → ffmpeg 转
  16k mono wav → **SenseVoice Small**（sherpa-onnx，经 sherpa-rs 绑定，纯 Rust
  进程内）+ silero VAD 分段转写，中文效果优于 Whisper，CPU 实时约 15-20 倍。

## 本地开发

```bash
# 后端（默认 127.0.0.1:8080，PORT 可覆盖）
cargo run -p server

# 前端（开发模式，/api 已代理到 8080）
cd frontend && npm install && npm run dev

# e2e 测试（需后端已启动）
cd e2e && python3 -m venv .venv && ./.venv/bin/pip install -r requirements.txt
./.venv/bin/python -m pytest -v
```

## 端点 ↔ e2e 对应

| 端点 | 测试文件 |
| --- | --- |
| `GET /api/health` | `e2e/endpoints/test_health.py` |
| `GET /api/tools`、`GET /api/tools/{id}/info` | `e2e/endpoints/test_tools.py` |
| `POST /api/auth/bili/qrcode`、`GET .../poll`、`GET /status`、`DELETE /` | `e2e/endpoints/test_auth_bili.py` |
| `POST /api/tools/bili2text/tracks`、`/extract` | `e2e/endpoints/test_extract.py` |

新增端点时同步新增对应测试文件（1:1 约定）。依赖登录态的完整链路用例
标 `@pytest.mark.live`，登录后 `pytest -m live` 手动跑。

## 路线图

- [x] workspace 骨架 + 工具注册制 + 门户页 + e2e 框架
- [x] 扫码登录（二维码生成/轮询/登录态落库 `data/credential.json`）
- [x] 字幕提取链路（BV/链接/短链解析 → 字幕列表 + 多语言 + 文本/SRT 导出）
- [ ] ASR 链路（yt-dlp 下载 → SenseVoice 转写 → 任务队列 + SSE 进度）
- [ ] 前端体验打磨（语言切换、批量提取）
