# bili2text · engram 档案索引

> 本段是 engram 档案的**指针快照**（权威在 engram，文档清单变化时同步更新本段）。
> 2026-09-30 由 init-project 建立，文档基线 HEAD `7480aa7`。

## 档案定位

- engram project：`bili2text`（id `01a0f14f-558c-7c22-b363-18239c549e84`，type=dev）
- codegraph：`bili2text`（cloud_index @ 7480aa7；查询前看 freshness，stale 先 sync）
- 仓库：<https://github.com/trtyr/bili2text>

## 文档清单（title → category → doc_id，一跳直达 doc_get）

| title | category | doc_id |
| --- | --- | --- |
| 项目概览与目标 | 概览 | `01a0f151-b353-7862-9684-7427731f230e` |
| 构建块视图 | 架构 | `01a0f152-aca6-7800-9e72-a4b52912b7e5` |
| 数据流与输出 | 架构 | `01a0f15a-af40-7341-bae6-40cd27f74cd7` |
| 运行时视图 | 架构 | `01a0f15c-1bf9-7761-938b-e06e6d3e0ad5` |
| 接口面 | 接口 | `01a0f153-c732-7122-80b8-88d68747acb6` |
| 测试门禁 | 测试 | `01a0f161-0476-7881-827a-745e045b07e2` |
| 技术决策记录 | 决策 | `01a0f161-ac06-76b2-b2ec-7cc120801964` |
| 更新记录 | 历史 | `01a0f162-019a-7263-9fd2-c497bc71a83e` |
| 术语表 | 术语 | `01a0f162-73c5-77c3-9a12-ecec5d643a6d` |

## 图清单（projects file_get 按名取）

- `system-context.html` — 系统上下文：CLI ↔ B 站接口 / 外部进程 / 模型 / 数据目录
- `building-blocks.html` — 构建块依赖：4 crate 依赖方向 + feature 门控
- `dataflow.html` — 数据流：字幕 / 转写 / 评论三管线汇入统一 Doc

## 检索配方（按意图直达）

- 架构怎么设计 / 模块职责 → 《构建块视图》《数据流与输出》
- 这个接口/命令是什么 → 《接口面》（CLI + 退出码 + B 站端点 + crate 间接口）
- 为什么这么定 → 《技术决策记录》（D1 去平台化…D7 多 P 提取）
- 有什么坑 / 遗留 → 《更新记录》遗留清单 + 《技术决策记录》附录漂移
- 术语（wbi / SESSDATA / 分 P / RTF）→ 《术语表》
- 怎么跑测试 → 《测试门禁》

## 更新纪律

文档清单变化 → 同步更新本段并 commit；仓库推进后 codegraph 陈旧 → 本机 `codegraph sync` + 服务端核对 freshness。
