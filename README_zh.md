[English](./README.md) | 中文

# TestVDB

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![Version](https://img.shields.io/badge/version-2.1.2-orange.svg)](https://github.com/yihui504/TestVDB/releases)

**基于 LLM 的向量数据库自动化缺陷挖掘工具**

TestVDB 以 Claude Code 插件形式运行，通过自然语言契约逆向工程从官方文档自动提取结构化约束，结合多 Agent 辩论机制在 Docker 沙箱中自动发现 Milvus、Qdrant、Weaviate、pgvector 的合规性缺陷，产出可复现、可追溯的完整证据链报告。

---

## v2.1.2 新特性

- **跨 Turn 状态机**： v3——支持 phase 级断点恢复。每个 phase 完成后立即持久化，上下文压缩后可从精确断点继续，不依赖模型记忆。
- **ScheduleWakeup Loop**：多轮挖掘采用  驱动的跨 Turn 迭代。每轮是独立 Turn， 在每轮开始时从磁盘状态文件重建完整流水线上下文。
- **上下文重建**：新增 ，读取 6 个状态文件（pipeline_state、mine_state、coverage、experience_handoff、structured_contract、threat_model），输出自包含的 Agent 上下文——当前 phase、已完成 phases、每 phase 产出摘要、全局进度、终止条件、下一步行动。
- **Executor 可靠性修复**： 的模板变量替换从嵌入 bash 命令改为 Step 0 显式 shell 赋值（、、）。bash 变量展开是确定性的——零字节日志 bug 根除。Agent 保持完整执行控制权（无脚本绕过）。
- **PostCompact 增强**： 同时支持 v3 和 legacy schema，输出精确 phase 级恢复指令并自动降级兼容。
- **Agent 更新**： 重写——4 步 SOP，每步显式声明变量，Windows 路径通过  标准化，每脚本实时 exit code 可见。

[完整更新日志 →](#v211-新特性)

---

## v2.1.1 新特性

- **质量加固**：所有攻击策略统一使用 `safe_request()` 包装器——零裸 API 调用，零连接/超时导致的脚本崩溃
- **AST 级 API 格式验证**：新增 `validate_api_format.py`，在 Stage 1 辩论中对攻击脚本进行 AST 级别检查，执行前拒绝裸 `.json()` 链式调用
- **Reporter 拆分**：`reporter.md`（缺陷报告）与 `reporter-mre.md`（MRE 脚本）分离——关注点分离，文件大小更合理
- **代码去重**：`_session_utils.py` 被 7 个 hook/维护脚本共享，消除约 100 行重复的 `_plugin_root()` / `is_session_locked()` 实现
- **嵌套派发禁令**：在所有 Agent prompt 中明确禁止嵌套 Agent 派发——这是发现并编码化的平台限制
- **Orchestrator 生命周期管理**：提取为 `orchestrator-lifecycle.md`（错误处理策略、PreCompact/PostCompact 上下文保护、进度可见性、多 DB 并行）
- **pgvector SQL 端点模式**：完成 `strategy_extractor.py` 中 SQL 数据库的 DDL/DML/Search/Index 正则模式
- **异常处理加固**：裸 `except:` → 具体异常类型、`--target` 缺值验证、`.env` 引号剥离
- **Agent 舰队**：18 个 Agent（+ `orchestrator-lifecycle`、+ `reporter-mre`），25 个脚本（+ `_session_utils.py`、+ `validate_api_format.py`、+ `reconstruct_context.py`）

[完整更新日志 →](#v21-新特性详解)

---

## v2.1 新特性

- **Phase 0：战略情报采集层**：在攻击流水线之前插入历史缺陷分析阶段，爬取目标仓库 GitHub Issues 和已合并 PR，构建威胁模型与认知盲点画像
- **Issue 三分类**：将历史 Issues 分为正样本（开发者承认的 bug）、负样本（by-design / wontfix）、无效样本（无回应），从正样本提取根因模式
- **开发者认知盲点模型**：5 类盲点体系（BS-01 ~ BS-05），将系统性开发者疏忽映射到攻击策略
- **跨 DB Bug Shape 迁移**：标记 Bug Shape 的 `cross_db_applicable` 属性，实现 Milvus→Qdrant→Weaviate→PGVector 的策略复用
- **3 个新 Agent**：`issue-miner`（raw）、`bug-shape-extractor`（redacted）、`threat-modeler`（redacted）——Agent 总数：18

[完整更新日志 →](#v20-新特性详解)

---

## 目录

- [v2.1.2 新特性](#v212-新特性)
- [v2.1.1 新特性](#v211-新特性)
- [v2.1 新特性](#v21-新特性)
- [v2.0 新特性详解](#v20-新特性详解)
- [项目概述](#项目概述)
- [缺陷分类体系](#缺陷分类体系)
- [快速开始](#快速开始)
- [安装方式](#安装方式)
- [使用方法](#使用方法)
- [架构设计](#架构设计)
- [目录结构](#目录结构)
- [配置说明](#配置说明)
- [环境要求](#环境要求)
- [辩论机制](#辩论机制)
- [输出产物](#输出产物)
- [许可证](#许可证)

---

## v2.0 新特性详解

### Fan-Out 攻击矩阵

v2.0 将攻击 Agent 从 3 个扩展为 **9 个并发 Agent**——每种攻击类型（边界/状态/语义）× 3 种 focus profile：

| Profile | 策略 |
|---------|------|
| `priority_first` | 优先测试高优先级约束 |
| `coverage_gap` | 从 `coverage.json` 中找覆盖率最低的端点 |
| `rejection_pattern` | 从上轮驳回模式反向推导，绕过已知驳回路径 |

9 个 Agent 全部并行派发，结果经 **3 级去重**（endpoint × constraint_id × strategy），独立验证奖励 confidence +0.1。

### 跨会话策略进化

挖掘策略现在**跨会话、跨数据库**持久化：
1. `strategy_extractor.py` — 每轮结束后提取有效攻击模式
2. `strategy_registry/` — 按 DB 存储策略（`{db}_strategies.json`）+ 全局注册表（`global_strategies.json`）
3. `strategy_injector.py` — 读取适用策略，注入到 Attack Agent 的 prompt 中
4. 跨 DB 迁移：Milvus 策略自动映射到 Qdrant 等价参数

所有进化记录审计在 `evolution_log.jsonl`。

### 7 模式 AI 故障检查

`scripts/ai_failure_check.py` 用 7 种检测模式验证流水线产出：

| 模式 | 检查内容 | 失败动作 |
|------|---------|---------|
| M1 | 脚本语法错误 | Rewind（回退重试） |
| M2 | 源 URL 可达性 | Reject（拒绝） |
| M3 | 执行结果数据验证 | Reject（拒绝） |
| M4 | `.done` 标记完整性（流水线完整性） | Halt（终止） |
| M5 | 缺陷分类一致性 | Rewind（回退重试） |
| M6 | 方法论编造检测 | Reject（拒绝） |
| M7 | 死循环检测 | Halt（终止） |

### Material Passport

每个 `structured_contract.json` 包含 `_passport` 字段，附带 SHA-256 哈希：

```json
{
  "_passport": {
    "schema_version": "2.0",
    "hash_algorithm": "sha256",
    "hash": "88ed0dc...",
    "endpoint_count": 68,
    "constraint_count": 39,
    "core_crud_coverage_pct": 95.0
  }
}
```

`scripts/passport_verify.py` 验证契约完整性：
- 退出码 0：PASS — 哈希匹配
- 退出码 1：NO_PASSPORT — 旧格式契约
- 退出码 2：TAMPERED — 被篡改，流水线拒绝并强制重新生成

### 数据访问分级

每个 Agent 文件在 frontmatter 中声明 `dataAccess` 级别：
- `raw` — 可访问原始文档和外部网络
- `redacted` — 仅访问特定会话产物
- `verified_only` — 仅访问经过 Judge 验证的数据

---

## 项目概述

TestVDB 是面向向量数据库的自动化缺陷挖掘工具，旨在解决传统数据库测试缺乏业务语义理解、以及大模型自动化测试极易产生"幻觉"的痛点。

**核心能力：**

- 从官方文档自动提取结构化契约（自然语言契约逆向工程）
- 基于契约自动生成针对性攻击测试脚本
- Docker 沙箱隔离执行，双轨策略（主机 Python / stdin 管道）
- 多 Agent 辩论机制过滤假阳性，保证缺陷可信度
- 三环证据链支撑，缺陷报告可追溯、可复现
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
/testvdb:mine weaviate 1.25.0 --min-defects 2
/testvdb:mine pgvector pg17 --max-rounds 0
```

---

## 安装方式

### Marketplace 安装（推荐）

TestVDB 作为 Claude Code 插件分发，通过 marketplace 安装：

```bash
# 在任意 Claude Code 会话中：
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

> **注意**：`--plugin-dir .` 仅在当前会话加载插件。修改文件后重启会话即可生效。

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

以下任一条件满足即终止挖掘循环：

1. **僵局**：连续 5 轮无新缺陷
2. **覆盖率**：契约覆盖率达到 95% 以上
3. **最大轮数**：达到 `--max-rounds` 设定值
4. **最低缺陷**：达到 `--min-defects` 设定值

### 多数据库并行

开多个终端窗口即可并行挖掘不同数据库：

```bash
# 终端 1
/testvdb:mine milvus v2.6.17
# 终端 2
/testvdb:mine qdrant v1.12.0
```

### 错误恢复

如果会话中断，重新执行相同命令即可恢复。系统会自动检测未完成的会话并从断点继续。

---

## 架构设计

### Agent 体系（16 个 Agent 类型 + 2 个辅助规范）

> **说明**：`plugin.json` 注册了 16 个 Agent 类型，可通过 `Agent(subagent_type="testvdb:xxx")` 派发。另外 2 个辅助规范（`orchestrator-lifecycle.md`、`reporter-mre.md`）分别定义了生命周期管理规则（由主进程消费）和 MRE 脚本生成（作为独立 Agent 注册）。

| Agent | dataAccess | 职责 |
|-------|-----------|------|
| **orchestrator** | redacted | 主编排器，协调全部子 Agent 完成流水线 |
| **orchestrator-lifecycle** | redacted | 生命周期管理：错误处理、Pre/PostCompact、进度可见性（从 orchestrator 提取） |
| **issue-miner** | raw | 爬取目标仓库历史 Issues 和已合并 PR，构建原始缺陷语料库 |
| **bug-shape-extractor** | redacted | 对历史 Issues 三分类（positive/negative/invalid），提取根因模式和开发者认知边界 |
| **threat-modeler** | redacted | 基于历史缺陷数据构建威胁模型和认知盲点模型，指导攻击优先级 |
| **knowledge-extractor** | raw | 从官方文档提取 API 知识 |
| **contract-formalizer** | redacted | 将原始知识形式化为结构化契约（含 `_passport`） |
| **attack-boundary** | redacted | 边界值攻击，测试参数边界约束 |
| **attack-state** | redacted | 状态攻击，测试状态一致性和逻辑违规 |
| **attack-semantic** | redacted | 语义攻击，测试语义层面的合规性 |
| **docker-executor** | redacted | 双轨策略执行脚本（主机 Python / Docker stdin pipe） |
| **judge-doc** | raw | 文档审查，验证缺陷的文档引用可达性与内容一致性（含网络验证） |
| **judge-evidence** | verified_only | 证据审查，判定缺陷证据可信度 |
| **judge-novelty** | raw | 新颖性审查，通过 GitHub 搜索判定缺陷是否为已知问题 |
| **judge-severity** | verified_only | 严重性评估，判定缺陷影响等级 |
| **reporter** | verified_only | 生成缺陷报告（含三环证据链） |
| **reporter-mre** | verified_only | 为确认的缺陷生成自包含 MRE 脚本 |
| **model-test** | redacted | CCSwitch 模型路由验证 |

### Skill 体系（4 个 Skill）

| Skill | 用途 |
|-------|------|
| **pipeline** | 缺陷挖掘流水线 SOP，定义六阶段执行流程 |
| **contract-schema** | 结构化契约 JSON Schema 参考 |
| **defect-taxonomy** | 四型缺陷分类法参考 |
| **docker-templates** | Docker 容器模板参考 |

### 数据流

```
Orchestrator
  |
  +--> [Phase 0: Strategic Intelligence — v2.1 新增]
  |     Issue Miner → Bug Shape Extractor → Threat Modeler
  |         |
  |         v
  |   threat_model.json (attack priorities + cognitive blindspots + judge enhancements)
  |
  +--> Knowledge Extractor --> raw_knowledge.md
  |                                    |
  +--> Contract Formalizer <-----------+
  |         |
  |         v
  |   structured_contract.json (_passport hash verified)
  |         |
  +--> Attack Trio (9 并发) <-- contract + reflection_context + threat_model + cross_session_strategies
  |   boundary×3 | state×3 | semantic×3
  |         |
  |         v
  |   test_scripts[] + debate_logs/stage1.json
  |         |
  +--> Executor <-- test_scripts[] (双轨: host Python / Docker stdin)
  |         |
  |         v
  |   execution_results[] + ai_failure_check (7 modes)
  |         |
  +--> Judge Quartet (并发) <-- execution_results[]
  |   doc (先行，权重调节) | evidence | novelty | severity
  |         |
  |         v
  |   confirmed_defects[] + debate_logs/stage2.json
  |         |
  +--> Reporter --> defect-N.md + summary.md + strategy_extractor
```

---

## 目录结构

```
TestVDB/
  .claude-plugin/plugin.json      插件清单（名称、版本、命令、Agent）
  .mcp.json                       MCP 服务器配置（GitHub API）
  agents/                         18 个 Agent 定义文件
    orchestrator.md
    orchestrator-lifecycle.md
    issue-miner.md
    bug-shape-extractor.md
    threat-modeler.md
    knowledge-extractor.md
    contract-formalizer.md
    attack-boundary.md
    attack-state.md
    attack-semantic.md
    docker-executor.md
    judge-doc.md
    judge-evidence.md
    judge-novelty.md
    judge-severity.md
    reporter.md
    reporter-mre.md
    model-test.md
  commands/mine.md                入口命令（/testvdb:mine）
  docker/                         Docker Compose 模板
    crawl4ai.yml                  Crawl4AI 网页抓取服务
    milvus.yml                    Milvus（etcd + MinIO + standalone）
    qdrant.yml                    Qdrant 单机
    weaviate.yml                  Weaviate 单机
    pgvector.yml                  PGVector 单机
  hooks/hooks.json                生命周期钩子（pre-compact、post-compact 等）
  skills/                         4 个 Skill 定义
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  intelligence/                   v2.1 战略情报缓存（per-DB，TTL 30 天）
  contracts/                      参考契约与配置 schema
    AGENTS.md
    settings_schema.json          配置验证 Schema
    pgvector_contract.json        PGVector 参考契约
    weaviate_contract.json        Weaviate 参考契约
  scripts/                        基础设施脚本（25 个）
    passport_verify.py            Material Passport 哈希验证
    strategy_extractor.py         跨会话策略提取
    strategy_injector.py          跨 DB 策略注入
    ai_failure_check.py           7 模式 AI 故障检查
    validate_api_format.py        AST 级 API 调用格式验证（v2.1.1）
    _session_utils.py             共享会话工具函数（v2.1.1）
    preflight.py                  会话预检
    crawl_fetch.py                Crawl4AI 网页抓取器（主方案）
    crawl_milvus.py               Milvus 专用文档爬虫
    hook_runner.py                跨平台钩子执行器
    github_search.py              GitHub 搜索工具
    prioritizer.py                攻击脚本优先级排序
    verify_defects.py             批量缺陷验证
    find_python.py                Python 解释器解析
    developer_attitude.py         开发者态度分析
    cleanup_stop.py               会话清理
    emergency_cleanup.py          紧急容器清理
    log_execution.py              执行日志记录
    notify_check.py               通知配置验证
    postcompact_verify.py         压缩后状态恢复
    precompact_save.py            压缩前状态保存
    retry_policy.py               重试策略报告
    gen_weaviate_contract.py      Weaviate 契约生成
    validate_weaviate_contract.py Weaviate 契约验证
  docs/                           文档
    reviews/                      代码审查报告
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

### .mcp.json

MCP 服务器配置，当前配置了 GitHub MCP 服务器，用于 Judge Novelty Agent 搜索已知缺陷以判定新颖性。需设置 `GITHUB_TOKEN` 环境变量。

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
| 网络访问 | -- | WebFetch 必须能访问目标文档站点（milvus.io、qdrant.tech 等） |
| GitHub Token | -- | 可选，用于新颖性判定（无则降级为 WebSearch） |

---

## 辩论机制

### Stage 1：攻击脚本同行评审

三个 Attack Agent 类型各 3 个 focus profile = 9 Agent 并发生成脚本，交叉审查（防止自评偏见）。通过审查的脚本进入执行阶段。

### Stage 2：Judge Quartet 投票

四个 Judge Agent 独立审查全部执行结果：

- **judge-doc**：先行执行，验证文档引用有效性，产出 DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH 作为权重调节器
- **judge-evidence**：证据门控，证据等级 D 则自动判定为非缺陷
- **judge-severity**：严重性门控，severity = trivial 则判定为非缺陷
- **judge-novelty**：新颖性标记（new / new_similar / already_reported），不参与缺陷确认投票，仅附加元数据

**缺陷确认规则：** evidence = is_defect AND severity = is_defect → 确认缺陷。

### 三环证据链

每个确认的缺陷必须包含完整的三环证据链：

1. **契约引用**：违反了哪条结构化契约约束（含 constraint ID）
2. **来源 URL**：约束提取自哪个官方文档页面
3. **文档链接**：相关文档的永久链接（可选：源代码引用）

---

## 输出产物

```
results/{target}/{version}/{timestamp}/
├── defects/                    # 缺陷报告（defect-1.md ... defect-N.md）
├── mre/                        # 自包含 MRE 复现脚本
├── summary.md                  # 汇总报告
├── debate_logs/                # 辩论日志
│   ├── stage1.json             # Stage 1 攻击脚本评审
│   └── stage2.json             # Stage 2 Judge 投票
├── structured_contract.json    # 结构化契约（含 _passport）
├── raw_knowledge.md            # 原始文档知识
├── mine_state.json             # 状态快照
├── coverage.json               # 覆盖率跟踪
├── session_metadata.json       # 会话元数据
└── experience_handoff.json     # 经验交接
```

---

## 许可证

[MIT](LICENSE)
