<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-06-06 -->

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

## What's New in v2.0

- **跨会话自进化**: 从 Milvus 挖掘中学到的策略自动迁移到 Qdrant/Weaviate/PGVector
- **Fan-Out Attack Trio**: 3 Agent × 3 seed = 9 并行生成流，策略多样性提升 3x
- **7-Mode AI Failure Checklist**: Reporter 自检 7 种 LLM 幻觉模式，造假→丢弃，违规→挂起
- **Material Passport**: 契约 sha256 防篡改 + 版本化追溯
- **data_access_level**: Agent 数据权限声明式标记

## Purpose
基于 LLM 的向量数据库自动化缺陷挖掘工具（Claude Code 插件）。通过自然语言契约逆向工程从官方文档提取结构化约束，结合 12 个 Agent 的 4-Judge 辩论机制，在 Docker 沙箱中自动发现向量数据库的合规性缺陷。支持 Milvus、Qdrant、Weaviate、PGVector 四种向量数据库。

## Key Files
| File | Description |
|------|-------------|
| `.claude-plugin/plugin.json` | 插件注册配置 |
| `settings.json` | 运行配置（端口、重试策略、流水线参数等） |
| `contracts/settings_schema.json` | 配置 JSON Schema 校验 |
| `contracts/db_strategies.json` | 各 DB 的策略配置（API 策略、攻击策略、文档源等） |
| `THEORETICAL_FRAMEWORK.md` | 理论框架文档（自然语言契约逆向工程 + 四型缺陷分类法） |
| `.mcp.json` | MCP Server 配置（GitHub Issues 搜索） |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `agents/` | 12 个 Agent 定义文件（orchestrator + knowledge-extractor + contract-formalizer + attack trio + docker-executor + judge quartet + reporter） |
| `skills/` | 4 个 Skill 文件（pipeline + contract-schema + defect-taxonomy + docker-templates） |
| `contracts/` | 结构化契约 JSON 文件 |
| `docker/` | Docker Compose 模板（milvus/qdrant/weaviate/pgvector/crawl4ai） |
| `scripts/` | 辅助 Python 脚本（crawl_fetch, preflight, hook_runner 等） |
| `commands/` | 用户命令定义（`/testvdb:mine`） |
| `results/` | 测试运行结果（按 target/version/timestamp 组织） |

## For AI Agents

### Working In This Directory
- 启动挖掘流水线：`/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]`
- 修改 Agent 行为前务必阅读 `THEORETICAL_FRAMEWORK.md` 理解四型缺陷分类法
- Docker 沙箱是测试执行的核心基础设施，所有探针在隔离容器中运行
- 12 个 Agent 通过文件系统（structured_contract.json, pipeline_state.json, debate_logs/*.json）通信

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
- 缺陷分类：Type-1（非法操作成功）、Type-2（诊断不足）、Type-3（运行时失败）、Type-4（状态/逻辑违规）
- 4-Judge 辩论：judge-doc（文档验证）+ judge-evidence（证据审查）+ judge-novelty（新颖性）+ judge-severity（严重性）

## Dependencies

### External
- Docker Engine + Docker Compose
- Python 3.9+ (httpx, html2text)
- Crawl4AI Docker 服务（自动启动）
- GitHub Token（可选，judge-novelty 的 MCP GitHub 搜索需要）
