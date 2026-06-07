<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-06-07 -->

# TestVDB

## Installation

```bash
# Marketplace (recommended)
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB

# Local development
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

## What's New in v2.1

- **Phase 0: 战略情报采集层**: 在攻击流水线之前插入历史缺陷分析阶段，从目标仓库的 Issues 和合并 PR 中提取根因模式和开发者认知盲点
- **Bug-Shape Extractor**: 三分类 Issue（positive/negative/invalid），提取根因模式，分析开发者认知边界
- **Threat Model + Cognitive Blindspot Model**: 基于历史数据构建威胁模型，定义"什么算漏洞、什么不算、为什么"，指导攻击方向和 Judge 判定
- **跨 DB 缺陷模式迁移**: 将历史 Bug Shape 标记为 `cross_db_applicable`，实现 Milvus→Qdrant→Weaviate→PGVector 的策略复用

## What's New in v2.0

- **跨会话自进化**: 从 Milvus 挖掘中学到的策略自动迁移到 Qdrant/Weaviate/PGVector
- **Fan-Out Attack Trio**: 3 Agent × 3 seed = 9 并行生成流，策略多样性提升 3x
- **7-Mode AI Failure Checklist**: Reporter 自检 7 种 LLM 幻觉模式，造假→丢弃，违规→挂起
- **Material Passport**: 契约 sha256 防篡改 + 版本化追溯
- **data_access_level**: Agent 数据权限声明式标记

## Purpose
基于 LLM 的向量数据库自动化缺陷挖掘工具（Claude Code 插件）。通过自然语言契约逆向工程从官方文档提取结构化约束，结合 16 个 Agent 的 4-Judge 辩论机制 + Phase 0 战略情报采集层，在 Docker 沙箱中自动发现向量数据库的合规性缺陷。支持 Milvus、Qdrant、Weaviate、PGVector 四种向量数据库。

## Key Files
| File | Description |
|------|-------------|
| `.claude-plugin/plugin.json` | 插件注册配置（16 agents + 4 skills + 1 command） |
| `settings.json` | 运行配置（端口、重试策略、流水线参数、intelligence 等） |
| `contracts/settings_schema.json` | 配置 JSON Schema 校验 |
| `contracts/db_strategies.json` | 各 DB 的策略配置（API 策略、攻击策略、文档源等） |
| `THEORETICAL_FRAMEWORK.md` | 理论框架文档（自然语言契约逆向工程 + 四型缺陷分类法 + 认知盲点理论） |
| `.mcp.json` | MCP Server 配置（GitHub Issues 搜索） |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `agents/` | 16 个 Agent 定义文件（Phase 0: issue-miner + bug-shape-extractor + threat-modeler / Phase 1: orchestrator + knowledge-extractor + contract-formalizer / Phase 2: attack-boundary + attack-state + attack-semantic / Phase 3: docker-executor / Phase 4: judge-doc + judge-evidence + judge-novelty + judge-severity / Phase 5: reporter / Aux: model-test） |
| `skills/` | 4 个 Skill 文件（pipeline + contract-schema + defect-taxonomy + docker-templates） |
| `contracts/` | 结构化契约 JSON 文件 |
| `docker/` | Docker Compose 模板（milvus/qdrant/weaviate/pgvector/crawl4ai） |
| `scripts/` | 辅助 Python 脚本（crawl_fetch, preflight, hook_runner 等） |
| `commands/` | 用户命令定义（`/testvdb:mine`） |
| `results/` | 测试运行结果（按 target/version/timestamp 组织） |
| `intelligence/` | **v2.1 新增** — Phase 0 战略情报数据（per-DB 缓存，Git 不跟踪；包含历史 bug shapes、威胁模型、认知盲点，TTL 30 天） |
| `issues/` | 已生成的缺陷报告存档（Markdown 格式，按 DB 组织） |
| `strategy_registry/` | 跨会话策略注册表（global + per-DB 策略，含 evolution 日志） |

## For AI Agents

### Working In This Directory
- 启动挖掘流水线：`/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]`
- **v2.1 战略情报缓存在 `intelligence/{target}/` 下，TTL 30 天（`intelligence.cache_ttl_hours`）**
- Phase 0 采集由 `intelligence.enabled` 控制，可独立开关不影响核心流水线
- 修改 Agent 行为前务必阅读 `THEORETICAL_FRAMEWORK.md` 理解四型缺陷分类法和认知盲点理论
- Docker 沙箱是测试执行的核心基础设施，所有探针在隔离容器中运行
- 16 个 Agent 通过文件系统（structured_contract.json, threat_model.json, pipeline_state.json, debate_logs/*.json）通信

### ⛔ 架构约束：子 Agent 无法可靠嵌套派发孙 Agent

**技术根因（2026-06-06 确认）：** Claude Code 插件体系中，子 Agent（如 orchestrator）
通过 `Agent` 工具派发孙 Agent 时，插件注册的 agent_type（如 `testvdb:knowledge-extractor`）
在孙 Agent 上下文中不可用，被记录为 `"unknown"` 类型。

**实证：**
- Session `20055422`: orchestrator 派发的子 Agent 类型全为 `"unknown"`
- Session `3e91e378`: orchestrator 6分钟完成，仅1个子Agent（类型 `unknown`），远少于预期的11+个子Agent

**架构决策：** 主进程直接担任编排者，按照 `agents/orchestrator.md` 的 SOP 逐步派发子 Agent。
主进程可以可靠地派发 `testvdb:*` 子 Agent。`testvdb:orchestrator` agent 类型保留为 SOP 参考文档。

**已发生的故障（2026-06-06）：** 主进程在启动 Orchestrator 时，在 prompt 中写入了
"Step 1: 创建目录, Step 2: 用 WebFetch 爬取文档, Step 3: 生成契约"，导致 Orchestrator
跳过 knowledge-extractor 和 contract-formalizer 的派发，直接自己做文档爬取。

**后果：**
- knowledge-extractor 内置的 Crawl4AI 优先策略未执行
- 文档版本验证逻辑被跳过
- contract-formalizer 的契约形式化流程被跳过
- 整个流水线偏离设计，产出质量下降

**正确做法：** 主进程按照 `commands/mine.md` 的 SOP 逐步派发子 Agent。
详见 `commands/mine.md` 的「架构约束」和「核心铁律」。
`agents/orchestrator.md` 是 SOP 参考文档，主进程按其规范执行编排。

### ⛔ ANTI-PATTERN: 主进程自己做子 Agent 的工作

**错误示例：** 主进程直接使用 WebSearch/WebFetch 爬取文档，或直接写 Python 攻击脚本。

**为什么错：**
- 跳过了子 Agent 内置的专业策略（如 knowledge-extractor 的 Crawl4AI 优先策略）
- 绕过了子 Agent 的验证步骤
- 流水线设计被破坏，产出质量下降

**正确做法：** 主进程只做编排（解析参数、检查缓存、更新状态文件），所有实质性工作
必须通过 `Agent(subagent_type="testvdb:xxx")` 派发给对应子 Agent。

### Testing Requirements
- 预检脚本：`python scripts/preflight.py`
- 集成测试需要 Docker 环境（`docker/*.yml`）
- 验证脚本位于 `scripts/verify/` 目录

### Common Patterns
- Orchestrator 使用 Agent 工具派发子 Agent（subagent_type="testvdb:xxx"）
- Agent 间通信通过 .done 标记文件确保写入原子性
- **v2.1 Phase 0 数据流**：issue-miner → bug-shape-extractor → threat-modeler → 产出注入 Attack/Judge Agent
- 缺陷分类：Type-1（非法操作成功）、Type-2（诊断不足）、Type-3（运行时失败）、Type-4（状态/逻辑违规）
- 4-Judge 辩论：judge-doc（文档验证）+ judge-evidence（证据审查）+ judge-novelty（新颖性）+ judge-severity（严重性）
- 认知盲点映射：BS-01(参数信任) → boundary attack, BS-02(错误消息) → semantic attack, BS-03(并发盲区) → state attack

### Error Log Conventions (v2.1)

为保持多 Agent 系统的一致性，所有 Agent 按以下约定记录错误：

| 位置 | 写入者 | 内容 |
|------|--------|------|
| `mine_state.json` → `error_log[]` | 主进程 (Orchestrator) | 流水线级别的错误（Agent 超时、产出缺失、门控失败） |
| `results/{target}/{version}/{timestamp}/error_log.json` | Reporter | 报告生成阶段的错误（复现失败、格式错误） |
| `session_metadata.json` → `errors[]` | 主进程 | 会话级别的汇总错误 |
| Agent 内部处理 | 各 Agent | 重试逻辑（静默重试 ≤3 次，超过后输出错误标记文件） |

**约定**：
- Agent 内部错误（网络重试、格式重试）不写全局 error_log，由 Agent 自行消化
- 只有跨 Agent 边界的问题（产出缺失、格式不可解析）才写全局 error_log
- 错误消息格式：`{agent_name}: {brief_description} (severity: {critical|high|medium|low})`

## Dependencies

### External
- Docker Engine + Docker Compose
- Python 3.9+ (httpx, html2text)
- Crawl4AI Docker 服务（自动启动）
- GitHub Token（可选，judge-novelty 的 MCP GitHub 搜索需要）
