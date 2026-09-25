<h1 align="center">bili2text</h1>

<p align="center"><strong>把 B 站视频变成文字：贴一个链接，拿到一份文档。</strong></p>

<p align="center">
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.85%2B-DEA584?logo=rust" alt="Rust"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue" alt="platform">
  <a href="https://crates.io/crates/bili2text"><img src="https://img.shields.io/crates/v/bili2text.svg" alt="crates.io"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-informational" alt="license"></a>
</p>

<p align="center">
  <a href="#这是什么">这是什么</a> ·
  <a href="#30-秒上手">30 秒上手</a> ·
  <a href="#它能做什么">它能做什么</a> ·
  <a href="#命令参考">命令参考</a> ·
  <a href="#已知限制">已知限制</a> ·
  <a href="docs/design.md">设计文档</a>
</p>

```text
$ bili2text https://www.bilibili.com/video/BV17pFLzXEEp/

✓ 职场地图 | 跳槽圣经，打工人如何润的更好？…（AI 字幕（ai-zh） · 527 段）
已保存：职场地图 _ 跳槽圣经….md
```

## 这是什么

bili2text 是一个把 B 站视频变成文字的命令行工具。看长视频没时间？
想让播客 / 访谈 / 教程变成可搜索的笔记？贴上链接：

- 有字幕（官方或 AI）→ **秒回**，直接提取成 Markdown
- 没字幕 → `--transcribe` 用本地 SenseVoice 模型转写，中文效果优于 Whisper，
  不上传任何音频

它是纯 CLI、零常驻服务：登录态、历史记录、转写模型全部留在本机，
一个数据目录 `~/.local/share/bili2text/` 管所有东西，删掉即卸载。

## 30 秒上手

前置：装好 [Rust 工具链](https://rustup.rs)。想用本地转写，安装时加
`--features transcribe`（多一步 C++ 编译，需要 cmake），运行时还需要
`yt-dlp`、`ffmpeg`——缺什么跑 `bili2text doctor --fix`，能自动补齐
（含 SenseVoice 模型下载，约 230MB 压缩包）。

```bash
cargo install bili2text                            # 字幕提取版（纯 Rust）
cargo install bili2text --features transcribe      # + 本地转写（需 cmake）
bili2text login                                    # 扫码登录（AI 字幕需要）
bili2text https://www.bilibili.com/video/BV17pFLzXEEp/
```

完成后当前目录会多出一个 `<视频标题>.md`——头部是视频信息表，
正文是全文。这就是全部流程。

## 它能做什么

#### 字幕提取（默认）

- 官方字幕优先、AI 字幕兜底，自动挑中文；`--lang` 可指定语言（如 `ai-zh`、`en-US`）
- 输出 Markdown：头部信息表（来源 / 时长 / 文本来源 / 段落数）+ 全文
- `--srt` 同时导出标准 SRT 字幕文件；`-o` 指定输出路径

#### 本地转写（`--transcribe`）

- 没字幕的视频也能转：yt-dlp 拉音轨 → ffmpeg 转码 → SenseVoice + VAD
  分段推理，全程本机
- 支持中 / 英 / 日 / 韩 / 粤
- 需要转写构建：`cargo install bili2text --features transcribe`
  （轻量构建下 `--transcribe` 会提示安装命令）

#### 账号与历史

- `bili2text login`：终端直接渲染二维码，B 站 App 扫一下就好
- `bili2text history`：提取 / 转写历史落在本机 SQLite，`show <id>` 回看全文，
  `rm <id>` 删除（id 支持前缀）

#### 脚本友好

- 按错误类别区分退出码（10 输入错误 / 31 无字幕 / 41 yt-dlp 失败…），
  完整表见 [docs/design.md](docs/design.md)
- 进度走 stderr，结果摘要走 stdout，产物路径明确打印

## 命令参考

```text
bili2text <BV号|链接|短链>            提取字幕，存为 <标题>.md
bili2text <输入> --transcribe         跳过字幕，本地转写（需转写构建）
bili2text <输入> --srt                同时导出 .srt
bili2text <输入> -o <路径>            指定输出文档路径
bili2text <输入> --lang <lan>         指定字幕语言（如 zh-Hans、ai-zh）

bili2text login                       扫码登录 B 站
bili2text status                      查看登录态
bili2text logout                      清除登录态

bili2text history                     历史列表
bili2text history show <id>           回看全文（id 支持前缀）
bili2text history rm <id>             删除记录
```

输入支持：裸 BV 号、视频页链接、b23.tv 短链（带查询参数也没关系）。

## 已知限制

- **AI 字幕与高音质下载需要登录**——扫码一次 10 秒解决，登录态长期留在本机
- **单视频单 P**：多 P / 合集暂不支持，是有意先不做（需求出现再说）
- **转写是 CPU 实时级**：约 0.7-1× 实时速度（M 系芯片），18 分钟视频约
  20 分钟；急用请优先想办法找字幕（且需转写构建，见上方安装）
- 番剧、课程等非普通视频页暂不支持

## 从源码构建

```bash
cargo build                                   # 构建（轻量，字幕提取）
cargo build --features transcribe             # + 本地转写（需 cmake）
cargo test                                    # 单元测试（轻量 18 个 / 转写构建 19 个）
cd e2e && ./.venv/bin/python -m pytest -v     # 端到端（Python；转写用例需转写构建，自动跳过）
```

架构、接口调研、退出码设计、数据目录布局等深度内容见
[docs/design.md](docs/design.md)。一句话版：workspace 四个 crate——
`bili2text`（CLI 应用）+ `bili-client` / `bili2text-downloader` /
`bili2text-asr`（能力库，crates.io 包名带 `bili2text-` 前缀，库名保持
`bili-client` / `downloader` / `asr`）。

---

<p align="center">
  <i>看不完的视频，就变成读得完的文字。</i><br>
  <a href="#30-秒上手">装上试试</a> · <a href="docs/design.md">设计文档</a> · <a href="https://github.com/trtyr/bili2text/issues">提 Issue</a><br>
  <small>以 <a href="LICENSE-MIT">MIT</a> 或 <a href="LICENSE-APACHE">Apache-2.0</a> 双许可开源</small>
</p>
