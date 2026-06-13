[English](./README.md) | 中文

# TestVDB

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![Version](https://img.shields.io/badge/version-2.1.3-orange.svg)](https://github.com/yihui504/TestVDB/releases)

**基于 LLM 的向量数据库自动化缺陷挖掘工具**

TestVDB 以 Claude Code 插件形式运行，通过自然语言契约逆向工程从官方文档自动提取结构化约束，结合多 Agent 辩论机制在 Docker 沙箱中自动发现 Milvus、Qdrant、Weaviate、pgvector 的合规性缺陷，产出可复现、可追溯的完整证据链报告。

---

## v2.1.3 新特性

- **反偷工减料门禁**：Stop-hook 流水线门禁（`scripts/hooks/pipeline_gate.py`）在会话结束时验证三个 LLM 偷工减料症状——（1）文档分析覆盖率低于 60% 阈值，（2）无理由的静默降级（fallback 缺少 FALLBACK_JUSTIFIED），（3）流水线未到达 DONE 阶段。攻击 Agent 受合约约束，必须产出包含 `raw_knowledge.md` Document Sources 精确 URL 的 `analyzed_documents_*.md` 文件，且每个 `FALLBACK_TRIGGERED` 必须配对 `[FALLBACK_JUSTIFIED: 理由]`。Gate 做精确字符串比对（非模糊匹配）——泛用或占位 URL 触发 exit 2 拦截。
- **Agent 合约强化**：三个攻击 Agent（`attack-boundary.md`、`attack-state.md`、`attack-semantic.md`）新增强制性步骤合约：(a) 写 analyzed_documents 前必须先 Read `raw_knowledge.md`，(b) 定位 `## Document Sources` 表格，(c) 从 `URL` 列逐字符复制。自检规则：每个 URL 必须与 Document Sources 表格中的行逐字符完全一致。
- **Gate 路径 Bug 修复**：`_resolve_round_dir()` 现正确基于 `project_root` 解析 `timestamp_dir`（pipeline v3 惯例），并回退到 `session_dir` 相对路径（legacy/test 惯例）。此前路径双重嵌套导致所有质量检查静默跳过。`_parse_analyzed_docs()` 现使用递归 glob（`rglob`）查找 `debate_logs/` 等子目录中的分析文档。
- **Gate 阈值可配置**：`TESTVDB_GATE_ACTIVE_THRESHOLD`（默认 600s）和 `TESTVDB_DOC_COVERAGE_THRESHOLD`（默认 0.6）现可通过环境变量配置。
- **项目清理**：移除 40+ 一次性开发脚本、空 JS 桩、临时 HTML/JSON 产物和过期 Docker 攻击脚本。参考数据迁移至 `data/`，运行日志归档至 `logs/development/`，分析流水线归类至 `scripts/analysis/`。

[完整更新日志 →](#v212-新特性)

---

## v2.1.2 新特性

- **跨 Turn 状态机**：`pipeline_state.json` v3——支持 phase 级断点恢复。每个 phase 完成后立即持久化，上下文压缩后可从精确断点继续。
- **ScheduleWakeup Loop**：多轮挖掘采用 ScheduleWakeup 驱动的跨 Turn 迭代。每轮是独立 Turn，`reconstruct_context.py` 在每轮开始时从磁盘状态文件重建完整流水线上下文。
- **上下文重建**：新增 `reconstruct_context.py`，读取 6 个状态文件，输出自包含的 Agent 上下文。
- **Executor 可靠性修复**：`docker-executor` 的模板变量替换改为 Step 0 显式 shell 赋值，bash 变量展开是确定性的——零字节日志 bug 根除。

---

## v2.1.1 新特性

- **质量加固**：所有攻击策略统一使用 `safe_request()` 包装器——零裸 API 调用
- **AST 级 API 格式验证**：新增 `validate_api_format.py`，Stage 1 辩论中 AST 级别检查攻击脚本
- **Reporter 拆分**：`reporter.md`（缺陷报告）与 `reporter-mre.md`（MRE 脚本）分离
- **代码去重**：`_session_utils.py` 被 7 个 hook/维护脚本共享
- **嵌套派发禁令**：在所有 Agent prompt 中明确禁止嵌套 Agent 派发
- **Agent 舰队**：18 个 Agent，25+ 脚本

---

## 目录

- [v2.1.3 新特性](#v213-新特性)
- [项目概述](#项目概述)
- [缺陷分类体系](#缺陷分类体系)
- [快速开始](#快速开始)
- [安装方式](#安装方式)
- [使用方法](#使用方法)
- [架构设计](#架构设计)
- [反偷工减料流水线门禁](#反偷工减料流水线门禁)
- [目录结构](#目录结构)
- [配置说明](#配置说明)
- [环境要求](#环境要求)
- [辩论机制](#辩论机制)
- [许可证](#许可证)

---

## 项目概述

TestVDB 是面向向量数据库的自动化缺陷挖掘工具，旨在解决传统数据库测试缺乏业务语义理解、以及大模型自动化测试极易产生"幻觉"（偷工减料、编造结果、静默跳过步骤）的痛点。

**核心能力：**

- 从官方文档自动提取结构化契约（自然语言契约逆向工程）
- 基于契约自动生成针对性攻击测试脚本
- Docker 沙箱隔离执行
- 多 Agent 辩论机制过滤假阳性，保证缺陷可信度
- 三环证据链支撑，缺陷报告可追溯、可复现
- **Stop-hook 反偷工减料门禁**，强制 Agent 诚实执行每一步
- 跨会话策略进化，经验跨 DB 迁移

**支持目标：**

| 目标数据库 | 协议 | Docker 复杂度 |
|-----------|------|--------------|
| Milvus | gRPC / REST | 高（etcd + MinIO + standalone） |
| Qdrant | REST / gRPC | 低（单容器） |
| Weaviate | REST / gRPC | 低（单容器） |
| pgvector | SQL | 低（单容器） |

---

## 缺陷分类体系

TestVDB 采用四型缺陷分类法（MECE 原则），确保缺陷判定的客观性和一致性：

| 类型 | 名称 | 定义 | 示例 |
|------|------|------|------|
| Type 1 | 非法操作成功 | 违反文档约束的输入被接受（2xx 而非 4xx） | `limit=-1` 返回 200 OK |
| Type 2 | 诊断信息不足 | 正确拒绝但错误消息不清晰 | 返回 "Unknown Error" 而非 "Invalid Dimension" |
| Type 3 | 运行时失败 | 合法输入导致崩溃、500 错误或异常行为 | 合法搜索请求返回 500 |
| Type 4 | 状态/逻辑违规 | API 返回成功但内部状态不一致 | INSERT 3 行但 COUNT 返回 2 |

### 分类决策树

```
1. 非法输入被接受？         → Type 1（非法操作成功）
2. 合法输入导致崩溃？       → Type 3（运行时失败）
3. 错误消息不清晰？         → Type 2（诊断信息不足）
4. 状态/结果不一致？        → Type 4（状态/逻辑违规）
5. 以上都不是               → 非缺陷
```

---

## 快速开始

### 1. 安装 Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. 安装 TestVDB 插件

**方式 A：Claude Code Marketplace（推荐）**
```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

**方式 B：本地克隆**
```bash
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

### 3. 启动挖掘

在 Claude Code 会话中使用 `/testvdb:mine` 命令：

```
/testvdb:mine milvus v2.6.17
/testvdb:mine qdrant v1.12.0 --max-rounds 3
/testvdb:mine weaviate 1.38.0 --min-defects 2
/testvdb:mine pgvector pg17 --max-rounds 0
```

---

## 安装方式

### Marketplace 安装（推荐）

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

插件全局安装，跨会话持久化。使用 `/help` 验证——应看到 `/testvdb:mine` 命令。

### 本地开发安装

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> **注意**：修改文件后重启会话即可生效。

---

## 使用方法

### 命令格式

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

### 参数说明

| 参数 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `<db>` | 是 | -- | 目标数据库：`milvus`、`qdrant`、`weaviate`、`pgvector` |
| `<version>` | 是 | -- | 目标版本号（如 `v2.6.17`、`v1.12.0`、`pg17`） |
| `--max-rounds N` | 否 | 5 | 最大挖掘轮数，设为 0 表示无上限 |
| `--min-defects N` | 否 | 1 | 最低缺陷产出要求 |

### 终止条件

1. **僵局**：连续 5 轮无新缺陷
2. **覆盖率**：契约覆盖率达到 95% 以上
3. **最大轮数**：达到 `--max-rounds` 设定值
4. **最低缺陷**：达到 `--min-defects` 设定值

### 多数据库并行

```bash
# 终端 1
/testvdb:mine milvus v2.6.17
# 终端 2
/testvdb:mine qdrant v1.12.0
```

### 错误恢复

如果会话中断，重新执行相同命令即可恢复。系统通过 `pipeline_state.json` 自动检测未完成的会话并从断点继续。

---

## 架构设计

### Agent 体系（18 个 Agent 类型）

| Agent | dataAccess | 职责 |
|-------|-----------|------|
| **orchestrator** | redacted | 主编排器，协调全部子 Agent 完成流水线 |
| **orchestrator-lifecycle** | redacted | 生命周期管理：错误处理、Pre/PostCompact、进度可见性 |
| **issue-miner** | raw | 爬取目标仓库历史 Issues 和已合并 PR |
| **bug-shape-extractor** | redacted | 对历史 Issues 三分类，提取根因模式 |
| **threat-modeler** | redacted | 基于历史缺陷数据构建威胁模型和认知盲点模型 |
| **knowledge-extractor** | raw | 从官方文档提取 API 知识 |
| **contract-formalizer** | redacted | 将原始知识形式化为结构化契约 |
| **attack-boundary** | redacted | 边界值攻击（含反偷工减料合约） |
| **attack-state** | redacted | 状态攻击（含反偷工减料合约） |
| **attack-semantic** | redacted | 语义攻击（含反偷工减料合约） |
| **docker-executor** | redacted | Docker 沙箱中批量执行脚本 |
| **judge-doc** | raw | 文档审查，验证缺陷的文档引用可达性 |
| **judge-evidence** | verified_only | 证据审查，判定缺陷证据可信度 |
| **judge-novelty** | raw | 新颖性审查，通过 GitHub 搜索判定缺陷是否为已知问题 |
| **judge-severity** | verified_only | 严重性评估，判定缺陷影响等级 |
| **reporter** | verified_only | 生成缺陷报告（含三环证据链） |
| **reporter-mre** | verified_only | 为确认的缺陷生成自包含 MRE 脚本 |
| **model-test** | redacted | 模型路由验证 |

### Skill 体系（4 个 Skill）

| Skill | 用途 |
|-------|------|
| **pipeline** | 缺陷挖掘流水线 SOP，定义六阶段执行流程 |
| **contract-schema** | 结构化契约 JSON Schema 参考 |
| **defect-taxonomy** | 四型缺陷分类法参考 |
| **docker-templates** | Docker 容器模板参考 |

### 数据流

```
主进程 (commands/mine.md)
  |
  +--> [Phase 0: 战略情报采集]
  |     Issue Miner → Bug Shape Extractor → Threat Modeler
  |         |
  |         v
  |   threat_model.json (攻击优先级 + 认知盲点 + Judge 增强)
  |
  +--> Knowledge Extractor --> raw_knowledge.md
  |                                    |
  +--> Contract Formalizer <-----------+
  |         |
  |         v
  |   structured_contract.json (_passport hash 验证)
  |         |
  +--> Attack Trio (9 并发) <-- contract + reflection_context + threat_model
  |   boundary×3 | state×3 | semantic×3
  |         |
  |         v
  |   test_scripts[] + analyzed_documents_*.md + debate_logs/stage1.json
  |         |
  +--> Docker Executor <-- test_scripts[]
  |         |
  |         v
  |   execution_results[]
  |         |
  +--> Judge Quartet (并发) <-- execution_results[]
  |   doc (先行，权重调节) | evidence | novelty | severity
  |         |
  |         v
  |   confirmed_defects[] + debate_logs/stage2.json
  |         |
  +--> Reporter --> defect-N.md + summary.md
            |
            v
  [Stop Hook] pipeline_gate.py -- 反偷工减料质量门禁
    ① 文档覆盖率 >= 60%
    ② fallback 全部有理由
    ③ phase = DONE
```

---

## 反偷工减料流水线门禁

TestVDB v2.1.3 引入 **Stop-hook 流水线门禁**，在会话结束时强制执行三个质量检查，防止 LLM Agent 静默偷工减料：

### 三个症状

| 症状 | 检查项 | Gate 行为 |
|------|--------|----------|
| ① 文档覆盖率 | 已分析文档 URL 与 `raw_knowledge.md` Document Sources 的比率 | < 60% → exit 2（拦截） |
| ② 降级理由 | 每个 `FALLBACK_TRIGGERED` 必须有 `[FALLBACK_JUSTIFIED: 理由]` | 无理由 → exit 2（拦截） |
| ③ 阶段完整性 | 流水线必须到达 `phase=DONE` | 未到达 → exit 2（拦截） |

### Agent 合约要求

每个攻击 Agent（`attack-boundary`、`attack-state`、`attack-semantic`）必须：

1. **先 Read**：写 analyzed_documents 前必须先 Read `raw_knowledge.md`
2. **定位表格**：找到 `## Document Sources` 表格
3. **逐字复制**：从 `URL` 列逐字符复制——gate 做精确字符串比对，非模糊匹配
4. **写文件**：输出 `analyzed_documents_{type}.md`，包含精确的文档源 URL
5. **自检**：每个 URL 必须与 Document Sources 表格中的行逐字符完全一致

### 配置

```bash
# Gate 活跃阈值（默认 600 秒）
export TESTVDB_GATE_ACTIVE_THRESHOLD=1200

# 文档覆盖率阈值（默认 0.6 = 60%）
export TESTVDB_DOC_COVERAGE_THRESHOLD=0.8
```

### Hook 注册

Gate 作为 Stop hook 注册在 `.claude/settings.local.json` 中：
```json
{
  "hooks": {
    "Stop": [{
      "matcher": "",
      "hooks": [{
        "type": "command",
        "command": "python scripts/hooks/pipeline_gate.py"
      }]
    }]
  }
}
```

---

## 目录结构

```
TestVDB/
  .claude-plugin/plugin.json      插件清单（名称、版本、命令、Agent）
  .claude/settings.local.json     Stop-hook 流水线门禁注册
  .mcp.json                       MCP 服务器配置（GitHub API）
  agents/                         21 个 Agent 定义文件
    orchestrator.md               主编排器 SOP
    orchestrator-lifecycle.md     生命周期管理规则
    issue-miner.md                历史 Issue 采集
    bug-shape-extractor.md        Issue 三分类
    threat-modeler.md             威胁模型构建
    knowledge-extractor.md        文档知识提取
    contract-formalizer.md        契约生成
    attack-boundary.md            边界值攻击（含反偷工减料合约）
    attack-state.md               状态攻击（含反偷工减料合约）
    attack-semantic.md            语义攻击（含反偷工减料合约）
    docker-executor.md            沙箱脚本执行器
    judge-doc.md                  文档引用验证
    judge-evidence.md             证据链验证
    judge-novelty.md              缺陷新颖性检查
    judge-severity.md             严重性评估
    reporter.md                   缺陷报告生成
    reporter-mre.md               MRE 脚本生成
    model-test.md                 模型路由验证
    _target_api_reference.md      契约驱动 API 参考（共享）
    api-template-formalizer.md    API 模板形式化
    dev-reviewer.md               Dev 审查 Agent
  commands/mine.md                入口命令（/testvdb:mine）
  docker/                         Docker Compose 模板
    crawl4ai.yml                  Crawl4AI 网页抓取服务
    milvus.yml                    Milvus（etcd + MinIO + standalone）
    qdrant.yml                    Qdrant 单机
    weaviate.yml                  Weaviate 单机
    pgvector.yml                  PGVector 单机
  skills/                         4 个 Skill 定义
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  intelligence/                   战略情报缓存（per-DB，TTL 30 天）
  contracts/                      参考契约与配置 schema
    settings_schema.json          配置验证 Schema
    pgvector_contract.json        PGVector 参考契约
    weaviate_contract.json        Weaviate 参考契约
  scripts/                        基础设施脚本
    hooks/
      pipeline_gate.py            Stop-hook 反偷工减料门禁（v2.1.3）
      _test_pipeline_gate.py      8 场景 gate 单元测试
      _test_stop_hook.py          Stop hook 集成测试
    preflight.py                  会话预检
    reconstruct_context.py        跨 Turn 上下文重建
    strategy_extractor.py         跨会话策略提取
    strategy_injector.py          跨 DB 策略注入
    threat_model_injector.py      威胁模型 prompt 注入
    passport_verify.py            Material Passport 哈希验证
    validate_api_format.py        AST 级 API 调用格式验证
    validate_weaviate_contract.py Weaviate 契约验证
    detect_risky_scripts.py       风险脚本检测（Stage 1 辩论）
    scan_script_errors.py         脚本错误扫描（打回触发）
    dedup_defects.py              跨轮次缺陷去重
    verify_defects.py             批量缺陷验证
    prioritize.py                 攻击脚本优先级排序
    developer_attitude.py         开发者态度分析
    crawl_fetch.py                Crawl4AI 网页抓取器（主方案）
    crawl_milvus.py               Milvus 专用文档爬虫
    github_search.py              GitHub 搜索工具
    find_python.py                Python 解释器解析
    hook_runner.py                跨平台钩子执行器
    retry_policy.py               重试策略报告
    _session_utils.py             共享会话工具函数
    analysis/                     参考分析流水线
      milvus_bug_shape_pipeline.py
      milvus_full_pipeline.py
    dev_review_repro.py           Dev 审查复现
    validate_threat_model.py      威胁模型验证
  data/                           参考数据
    weaviate_openapi_schema.json  Weaviate OpenAPI Schema
    experience_handoff.json       经验交接模板
  logs/development/               开发运行日志（归档）
  strategy_registry/              跨会话攻击策略
  docs/                           文档
    reviews/                      代码审查报告
    acceptance-checklist-v2.1.1.md
  tests/                          测试套件
  settings.json                   插件配置（26+ 可配置参数）
  AGENTS.md                       Agent 编排规则
  THEORETICAL_FRAMEWORK.md        理论框架论文
  LICENSE                         MIT 许可证
```

---

## 配置说明

### settings.json

主配置文件，包含以下配置分组：

| 分组 | 关键参数 | 说明 |
|------|---------|------|
| `docker` | `cleanup_on_exit`、`startup_timeout_seconds`、各 DB 端口 | Docker 容器生命周期与端口映射 |
| `github` | `token` | GitHub PAT，用于新颖性判定 |
| `retry` | `max_attempts`、`*_delay_seconds` | 重试与延时策略 |
| `pipeline` | `default_max_rounds`、`default_min_defects` | 流水线执行限制 |
| `results` | `base_dir`、`max_sessions` | 输出目录与会话管理 |
| `knowledge` | `cache_enabled`、`cache_ttl_hours` | 契约缓存（默认 168h / 7 天） |
| `notification` | `on_severity`、`webhook_url` | 严重缺陷告警 |
| `network` | `proxy` | HTTP 代理 |
| `evolution` | `enabled`、`strategy_registry_dir`、`max_strategies_per_injection`、`min_confidence_for_injection` | 跨会话策略进化 |
| `fan_out` | `enabled`、`seeds_per_agent`、`profiles` | Fan-Out 攻击矩阵（9 并发 Agent） |
| `ai_failure_check` | `enabled`、`halt_on`、`reject_on`、`rewind_on` | 7 模式 AI 故障检测 |
| `material_passport` | `enabled`、`hash_algorithm`、`reject_on_tamper` | 契约哈希完整性验证 |
| `intelligence` | `enabled`、`cache_ttl_hours`、`time_window_months`、`max_issues`、`max_commits`、`inject_to_attack_agents`、`inject_to_judge_agents` | v2.1 Phase 0 战略情报采集层配置 |

---

## 环境要求

| 依赖 | 最低版本 | 说明 |
|------|---------|------|
| **LLM 模型** | Claude Sonnet/Opus | 通过 Claude Code 运行 |
| Claude Code CLI | 最新 | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20.10+ | 运行中，用于沙箱隔离 |
| Python | 3.9+ | 低于 3.9 为致命错误，流水线将终止 |
| 磁盘空间 | 10GB+ | Docker 镜像与结果存储 |
| Docker Hub Token | -- | **建议**。设置 `DOCKER_HUB_TOKEN` 环境变量以提升速率限制 |
| 网络访问 | -- | WebFetch 必须能访问目标文档站点 |
| GitHub Token | -- | 可选，用于新颖性判定 |

---

## 辩论机制

### Stage 1：攻击脚本同行评审

三个 Attack Agent 类型各 3 个 focus profile = 9 Agent 并发生成脚本，交叉审查（防止自评偏见）。`detect_risky_scripts.py` 和 `validate_api_format.py` 在进入执行阶段前进行自动化检查。

### Stage 2：Judge Quartet 投票

四个 Judge Agent 独立审查全部执行结果：

- **judge-doc**：先行执行，验证文档引用有效性，产出 DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH 作为权重调节器
- **judge-evidence**：证据门控，证据等级 D 则自动判定为非缺陷
- **judge-severity**：严重性门控，severity = trivial 则判定为非缺陷
- **judge-novelty**：新颖性标记，不参与缺陷确认投票，仅附加元数据

**缺陷确认规则：** evidence = is_defect AND severity = is_defect → 确认缺陷。

### 三环证据链

每个确认的缺陷必须包含完整的三环证据链：

1. **契约引用**：违反了哪条结构化契约约束（含 constraint ID）
2. **来源 URL**：约束提取自哪个官方文档页面
3. **文档链接**：相关文档的永久链接（可选：源代码引用）

---

## 许可证

[MIT](LICENSE)
