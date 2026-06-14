# TestVDB

[English](./README.md) | 中文

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code 插件](https://img.shields.io/badge/Claude%20Code-插件-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![版本](https://img.shields.io/badge/version-2.2.0-orange.svg)](https://github.com/yihui504/TestVDB/releases)
[![测试](https://img.shields.io/badge/tests-55%20passed-brightgreen.svg)](tests/)

**向量数据库自动化缺陷挖掘**

TestVDB 是一个基于 LLM 的 Claude Code 插件，自动发现向量数据库中的合规性缺陷。它从官方文档逆向工程出结构化契约，通过多 Agent 辩论生成针对性攻击脚本，在 Docker 沙箱中执行，并产出带完整证据链的已验证缺陷报告。

当前支持 **Milvus**、**Qdrant**、**Weaviate**、**pgvector** 四种向量数据库。

---

## v2.2.0 新特性 — 命令解耦

原本单一的 `/testvdb:mine` 流水线已拆成**三个可独立触发、智能协作的命令**：

| 命令 | 阶段 | 产出 |
|------|------|------|
| `/testvdb:contract <db> <version> [--force]` | 文档提取 + 契约生成 | `structured_contract.json` |
| `/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]` | 情报采集 + 威胁建模 | `threat_model.json` |
| `/testvdb:mine <db> <version> [--intel \| --contract] [...]` | 攻击挖掘（智能消费 intel/contract 缓存） | 缺陷 + 报告 |

**智能缓存复用（D 判断）** —— `scripts/check_cache.py` 通过四条件判断是否复用缓存的 intel/contract：存在 → TTL 新鲜 → 有效 → target/version 匹配。任一不满足 → 重新生成；全满足 → 纯挖掘（跳过生成，省时）。

**`--intel`/`--contract` 参数控制** 与 **C 边界** 语义：

| 缓存状态 | `--xxx false` 行为 |
|---------|-------------------|
| **MISSING**（无缓存） | 报错退出（"缺失，请先 `/testvdb:xxx`"） |
| **STALE / INVALID**（过期/无效） | 用现有 + 警告（不刷新） |
| **USABLE** | 正常使用 |

这区分了"我有但想用旧的"与"压根没有"——避免在没有前置产物时静默挖掘。

**端到端验证**（CC 2.1.165）：5 种 Agent 全部派发成功，零 `unknown`——knowledge-extractor、contract-formalizer、issue-miner、bug-shape-extractor、threat-modeler。

[完整更新日志 →](#v213-新特性)

---

## v2.1.3 新特性

- **反偷工减料强制执行**：Stop hook 流水线门禁（`scripts/hooks/pipeline_gate.py`）在会话结束时校验三个 LLM 偷工减料症状——(1) 文档分析覆盖率低于阈值、(2) 无理由的 fallback、(3) 流水线阶段未达 DONE。门禁做精确字符串匹配（非模糊）——通用或占位 URL 会被 exit 2 拦截。
- **Agent 契约强化**：三个攻击 Agent 均含强制分步契约——读 `raw_knowledge.md` → 定位 `## Document Sources` 表 → 逐字符复制 URL。
- **门禁路径 bug 修复**：`_resolve_round_dir()` 正确按 `project_root`（pipeline v3 约定）解析 `timestamp_dir`，并回退到 `session_dir` 相对路径。
- **门禁阈值可配置**：`TESTVDB_GATE_ACTIVE_THRESHOLD`（默认 600s）和 `TESTVDB_DOC_COVERAGE_THRESHOLD`（默认 0.6）支持环境变量配置。

---

## v2.1.2 新特性

- **跨 Turn 状态机**：`pipeline_state.json` v3——上下文压缩后 phase 级断点恢复。
- **ScheduleWakeup 循环**：多轮挖掘用 `ScheduleWakeup` 驱动的跨 Turn 迭代；`reconstruct_context.py` 每轮从磁盘状态重建完整上下文。
- **Executor 可靠性修复**：模板变量替换从内嵌 bash 移到显式 Step 0 shell 赋值——消除零字节日志 bug。

---

## v2.1.1 新特性

- **质量加固**：所有攻击脚本使用 `safe_request()` 模式——零裸 API 调用。
- **AST 级 API 格式校验**：Stage 1 辩论中的 `validate_api_format.py`。
- **Target 中立性校验**：`validate_target_neutrality.py` 确保攻击脚本不泄露 DB 特定签名（如 target=weaviate 但脚本命中 Qdrant 端口 `6333`）。
- **Reporter 拆分**：`reporter.md`（缺陷报告）从 `reporter-mre.md`（MRE 脚本）拆出。

---

## 目录

- [新特性](#v22-新特性--命令解耦)
- [工作原理](#工作原理)
- [缺陷分类体系](#缺陷分类体系)
- [快速开始](#快速开始)
- [安装方式](#安装方式)
- [使用方法（三命令）](#使用方法三命令)
- [架构设计](#架构设计)
- [反偷工减料流水线门禁](#反偷工减料流水线门禁)
- [目录结构](#目录结构)
- [配置说明](#配置说明)
- [环境要求](#环境要求)
- [证据链标准](#证据链标准)
- [许可证](#许可证)

---

## 工作原理

TestVDB 是一个 **Claude Code 插件**，由专业化 Agent 编排。自 v2.2.0 起，流水线暴露**三个解耦命令**，可独立运行或通过智能缓存复用组合：

```
┌─────────────────┐     ┌──────────────────┐
│ /testvdb:intel  │     │ /testvdb:contract│
│ issue-miner     │     │ knowledge-       │
│ bug-shape       │     │ extractor        │
│ threat-modeler  │     │ contract-        │
│   ↓             │     │ formalizer       │
│ threat_model.json│    │   ↓              │
│ (缓存 30 天)    │     │ structured_      │
└─────────────────┘     │ contract.json    │
         │              │ (缓存 7 天)      │
         │   D 判断      └──────────────────┘
         │   (check_cache.py)        │
         └──────────┬───────────────┘
                    ▼
         ┌─────────────────────┐
         │ /testvdb:mine       │  ← 智能消费缓存的
         │ attack-boundary/    │     intel + contract（新鲜则跳过生成）
         │ state/semantic      │
         │ docker-executor     │
         │ judge-* (4)         │
         │ reporter            │
         │   ↓                 │
         │ 缺陷 + MRE          │
         └─────────────────────┘
```

**挖掘轮次**使用 **ScheduleWakeup 驱动的跨 Turn 迭代**——每轮是独立 Turn，`pipeline_state.json`（v3 状态机）持久化 phase 级进度以精确断点恢复。**Stop hook 流水线门禁**在会话结束时强制反偷工减料质量检查。

每轮将上一轮的 `reflection_context` 注入攻击 Agent，实现策略自适应。Phase 0 情报（威胁模型 + 认知盲点）优先攻击历史上缺陷密度高的攻击面。

---

## 缺陷分类体系

TestVDB 将发现的缺陷分为四类 MECE（互斥且完备）类别：

| 类型 | 名称 | 定义 | 示例 |
|------|------|------|------|
| 类型 1 | 非法成功 | 违反文档约束的输入被接受 | `limit=-1` 返回 200 OK |
| 类型 2 | 诊断差 | 非法输入被拒，但错误信息不清晰 | "Unknown Error" 而非 "Invalid Dimension" |
| 类型 3 | 运行时失败 | 合法输入导致崩溃或 500 | 合法搜索返回 500 |
| 类型 4 | 状态/逻辑违规 | API 返回成功，但内部状态不一致 | INSERT 3 行，COUNT 返回 2 |

```
1. 非法输入被接受？       --> 类型 1
2. 合法输入导致崩溃？     --> 类型 3
3. 错误信息不清晰？       --> 类型 2
4. 状态/结果不一致？      --> 类型 4
5. 以上都不是             --> 非缺陷
```

---

## 快速开始

### 1. 安装 Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. 安装 TestVDB 插件

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### 3a. 完整挖掘（默认——向后兼容）

```
/testvdb:mine milvus v2.6.17
/testvdb:mine qdrant v1.12.0 --max-rounds 3
```

`mine` 自动检测缓存新鲜度（D 判断）——若 intel/contract 新鲜，跳过生成直接挖掘。

### 3b. 阶段独立命令（v2.2.0 新增）

```
# 仅生成/刷新契约（不挖掘）——调试 contract-formalizer
/testvdb:contract weaviate 1.38.0

# 仅采集情报（不生成契约/挖掘）——刷新威胁模型
/testvdb:intel pgvector --max-issues 50 --max-commits 20

# 强制重新生成契约，然后挖掘
/testvdb:mine milvus v2.6.17 --contract true
```

---

## 安装方式

### Marketplace 安装（推荐）

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### 本地开发安装

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> 文件变更在下一个会话生效。

---

## 使用方法（三命令）

### `/testvdb:contract` — 文档提取 + 契约生成

```
/testvdb:contract <db> <version> [--force]
```

| 参数 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `<db>` | 是 | — | `milvus`、`qdrant`、`weaviate`、`pgvector` |
| `<version>` | 是 | — | 目标版本（如 `1.38.0`） |
| `--force` | 否 | — | 强制重新生成，忽略缓存 |

**只跑** knowledge-extractor → contract-formalizer → 门控校验。不启动攻击/执行/judge/报告。缓存 TTL：`knowledge.cache_ttl_hours`（默认 168h / 7 天）。

### `/testvdb:intel` — 情报采集 + 威胁建模

```
/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]
```

| 参数 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `<db>` | 是 | — | `milvus`、`qdrant`、`weaviate`、`pgvector` |
| `--max-issues N` | 否 | settings `intelligence.max_issues`（500） | 采集最近 N 条 issue + 已合并 PR |
| `--max-commits N` | 否 | settings `intelligence.max_commits`（200） | 采集最近 N 个 commit |
| `--force` | 否 | — | 强制重新采集，忽略缓存 |

**只跑** issue-miner → bug-shape-extractor → threat-modeler。情报是 **per-target**（不限版本）。缓存 TTL：`intelligence.cache_ttl_hours`（默认 720h / 30 天）。

### `/testvdb:mine` — 攻击挖掘（智能消费）

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N] [--intel true|false] [--contract true|false]
```

| 参数 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `<db>` | 是 | — | `milvus`、`qdrant`、`weaviate`、`pgvector` |
| `<version>` | 是 | — | 目标版本 |
| `--max-rounds N` | 否 | 5 | 最大挖掘轮数。`0` = 无上限 |
| `--min-defects N` | 否 | 1 | 最低缺陷产出要求 |
| `--intel true\|false` | 否 | `auto` | 情报阶段控制（见下方 C 边界） |
| `--contract true\|false` | 否 | `auto` | 契约阶段控制（见下方 C 边界） |

**`auto`（默认）**：通过 `check_cache.py` 做 D 判断——USABLE→跳过生成（纯挖掘）；MISSING/STALE/INVALID→重新生成；MISMATCH→报错。

**`true`**：强制重新生成（绕过缓存）。

**`false`（C 边界）**：MISSING→报错退出；STALE/INVALID→用现有 + 警告；USABLE→正常使用。

### 终止条件（挖掘）

1. **僵局**：连续 5 轮无新缺陷
2. **覆盖率**：契约覆盖率 ≥ 95%
3. **最大轮数**：达到 `--max-rounds`
4. **最低缺陷**：达到 `--min-defects`

### 错误恢复

重新运行相同命令可恢复。系统通过 `pipeline_state.json` 自动检测未完成会话。

### 产出结构

```
results/{db}/{version}/{timestamp}/
  defects/defect-1.md              # 缺陷报告
  mre/defect-1-script.py           # 最小可复现示例
  summary.md                       # 会话汇总
  debate_logs/                     # Stage 1 + Stage 2 辩论日志
  structured_contract.json         # 生成的契约（含 passport）
  pipeline_state.json              # v3 跨 Turn 状态机
  coverage.json                    # 端点覆盖率跟踪
  experience_handoff.json          # 跨轮反思上下文

intelligence/{target}/             # 战略情报（per-DB，TTL 30 天）
  threat_model.json                # 威胁模型 + 认知盲点
  issue_corpus.json / bug_shapes.json / ...  # 中间产物
```

---

## 架构设计

### Agent 体系（17 个核心 Agent）

| Agent | dataAccess | 角色 |
|-------|-----------|------|
| **orchestrator** | redacted | 流水线编排 SOP（主进程直接派发） |
| **issue-miner** | raw | 爬取历史 issue 和已合并 PR |
| **bug-shape-extractor** | redacted | issue 三分类，提取根因模式 |
| **threat-modeler** | redacted | 构建威胁模型和认知盲点 |
| **knowledge-extractor** | raw | 爬取官方文档，提取端点/约束 |
| **contract-formalizer** | redacted | 原始知识 → 结构化 JSON 契约 |
| **attack-boundary** | redacted | 边界值攻击脚本（契约驱动，target 中立） |
| **attack-state** | redacted | 状态转换攻击脚本 |
| **attack-semantic** | redacted | 语义/逻辑攻击脚本 |
| **docker-executor** | redacted | Docker 沙箱批量脚本执行 |
| **judge-doc** | raw | 文档引用校验（权重调节器） |
| **judge-evidence** | verified_only | 证据链完整性 |
| **judge-novelty** | raw | 通过 GitHub 搜索查缺陷新颖性 |
| **judge-severity** | verified_only | 严重性评估 |
| **reporter** | verified_only | 缺陷报告生成 |
| **reporter-mre** | verified_only | 自包含 MRE 脚本生成 |
| **model-test** | redacted | 模型路由验证 |

> 另有辅助定义：`orchestrator-lifecycle`（生命周期管理）、`dev-reviewer`、`api-template-formalizer`、`_target_api_reference`（共享的契约驱动 API 参考）。

### Skill 体系（4 个）

| Skill | 用途 |
|-------|------|
| **pipeline** | 6 阶段流水线 SOP |
| **contract-schema** | 契约形式化的 JSON schema 参考 |
| **defect-taxonomy** | 四类缺陷分类参考 |
| **docker-templates** | 各目标 DB 的 Docker 容器模板 |

### 两阶段辩论机制

**Stage 1 — 攻击脚本同行评审**：攻击 Agent 生成测试脚本；脚本在沙箱执行前经自动化评审（去重、AST 校验、target 中立性检查、风险模式检测）。

**Stage 2 — Judge Quartet 投票**：四个 judge Agent 评审结果。`judge-doc` 先跑作为权重调节器（DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH）调整其他三个 judge 的严格度。当 evidence 和 severity 都投 `is_defect` 时确认缺陷。

---

## 反偷工减料流水线门禁

**Stop hook 流水线门禁**在会话结束时强制三个质量症状，防止 LLM Agent 静默偷工减料：

| 症状 | 检查 | 门禁行为 |
|------|------|---------|
| ① 文档覆盖率 | 已分析 URL vs `raw_knowledge.md` Document Sources | < 阈值 → exit 2（拦截） |
| ② Fallback 理由 | 每个 `FALLBACK_TRIGGERED` 需配 `[FALLBACK_JUSTIFIED: reason]` | 无理由 → exit 2（拦截） |
| ③ 阶段完整度 | 流水线须达 `phase=DONE` | 未 DONE → exit 2（拦截） |

```bash
# 可配置阈值
export TESTVDB_GATE_ACTIVE_THRESHOLD=1200    # 默认 600s
export TESTVDB_DOC_COVERAGE_THRESHOLD=0.8    # 默认 0.6
```

---

## 目录结构

```
TestVDB/
  .claude-plugin/plugin.json      插件清单（v2.2.0）
  agents/                         17 个核心 + 辅助 Agent 定义
  commands/                       3 个命令（v2.2.0 解耦）
    mine.md                         智能挖掘（消费 intel/contract 缓存）
    contract.md                     独立契约生成
    intel.md                        独立情报采集
  skills/                         4 个 skill 定义
  scripts/                        基础设施脚本（32 个模块）
    check_cache.py                  v2.2.0 D 判断（缓存复用检测）
    hooks/pipeline_gate.py          Stop hook 反偷工减料门禁
    preflight.py / reconstruct_context.py / validate_contract.py / ...
    validate_target_neutrality.py   target 中立攻击校验（v2.1.1）
  docker/                         Docker Compose 模板（5 DB + crawl4ai）
  contracts/                      参考契约 + settings schema
  intelligence/                   战略情报缓存（per-DB，TTL 30 天）
  strategy_registry/              跨会话攻击策略
  tests/                          测试套件（55 passed, 1 skipped）
  docs/                           spec + plan + 评审报告
  settings.json                   插件配置（26+ 参数）
  AGENTS.md                       Agent 编排规则
  THEORETICAL_FRAMEWORK.md        研究论文
```

---

## 配置说明

### settings.json

关键配置节：

| 节 | 关键参数 | 说明 |
|----|---------|------|
| `docker` | `cleanup_on_exit`、各 DB 端口 | 容器生命周期和端口映射 |
| `pipeline` | `default_max_rounds`、`default_min_defects` | 流水线执行限制 |
| `knowledge` | `cache_enabled`、`cache_ttl_hours` | 契约缓存（默认 168h） |
| `intelligence` | `enabled`、`cache_ttl_hours`、`max_issues`、`max_commits`、`inject_to_*` | 战略情报（默认 720h TTL） |
| `evolution` | `enabled`、`strategy_registry_dir` | 跨会话策略进化 |
| `fan_out` | `enabled`、`seeds_per_agent`、`profiles` | Fan-Out 攻击派发（9 并发） |
| `material_passport` | `enabled`、`hash_algorithm`、`reject_on_tamper` | 契约哈希完整性 |
| `ai_failure_check` | `enabled`、`halt_on`、`reject_on` | 7 模式 AI 失败检测 |

### .mcp.json

配置 novelty judge 使用的 GitHub MCP 服务器。

---

## 环境要求

| 要求 | 版本 | 说明 |
|------|------|------|
| **LLM 模型** | Claude Sonnet/Opus | 通过 Claude Code 运行 |
| Claude Code CLI | 最新 | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20+ | 流水线启动前必须运行 |
| Python | 3.9+ | hook 和辅助脚本使用 |
| 磁盘空间 | 10GB+ | Docker 镜像和结果 |
| Docker Hub Token | — | **推荐**以获得更高速率限制 |
| 网络访问 | — | 须能访问目标文档站点 |
| GitHub Token | — | 可选；启用完整 novelty judge |

> **CC 版本说明**：子 Agent 派发在部分 proxy 环境下需要 Claude Code 2.1.165（v2.1.166+ 在某些 proxy 下可能不注入 Task/Agent 工具）。若派发返回 `unknown`，将 CC 固定到 2.1.165。

---

## 证据链标准

每个确认的缺陷须满足**三环证据链**：

1. **契约引用**：违反的具体约束，带结构化契约中的 constraint ID
2. **源 URL**：定义该约束的官方文档页面直链
3. **文档链接**：（可选）源码引用或 GitHub issue

每个缺陷报告含**最小可复现示例（MRE）**——可在全新 Docker 容器中复现的自包含 Python 脚本。

---

## 许可证

本项目基于 [MIT 许可证](LICENSE)。
