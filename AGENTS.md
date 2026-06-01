<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# TestVDB

## Purpose
基于 LLM 的向量数据库自动化缺陷挖掘工具。通过自然语言契约逆向工程从官方文档提取结构化约束，结合 9 种确定性测试生成器与 LLM 编排器，在 Docker 沙箱中自动发现向量数据库的合规性缺陷。支持 Milvus、Qdrant、Weaviate、PGVector 四种向量数据库。

## Key Files
| File | Description |
|------|-------------|
| `Cargo.toml` | Rust 项目依赖配置（edition 2024，核心依赖：tokio, reqwest, serde, clap） |
| `Cargo.lock` | 依赖锁定文件 |
| `PLAN.md` | 项目开发计划（Phase A-D + Harness 加固） |
| `THEORETICAL_FRAMEWORK.md` | 理论框架文档（自然语言契约逆向工程 + 四型缺陷分类法） |
| `.gitignore` | Git 忽略规则 |
| `.cargo/config.toml` | Cargo 编译配置 |
| `docker-compose.*.yml` | 各目标 DB 的 Docker Compose 配置（milvus/qdrant/weaviate/pgvector） |
| `cleanup.ps1` | Docker 资源清理脚本 |
| `testvdb_baseline.json` | 基线测试数据 |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Rust 源代码（详见 `src/AGENTS.md`） |
| `contracts/` | 各目标 DB 的结构化契约 JSON 文件（详见 `contracts/AGENTS.md`） |
| `issues/` | 已发现的缺陷报告（Markdown + JSON，详见 `issues/AGENTS.md`） |
| `results/` | 测试运行结果（按 target/version/timestamp 组织，详见 `results/AGENTS.md`） |
| `shadow_mode_results/` | Shadow Mode 对比结果（详见 `shadow_mode_results/AGENTS.md`） |
| `.trae/` | Trae IDE 配置（plans/specs/state/auto_contracts/endpoints） |

## For AI Agents

### Working In This Directory
- 构建命令：`cargo build`；测试命令：`cargo test`
- 运行命令示例：`cargo run -- mine --target qdrant --version v1.13.0 --shadow --skip-verify`
- 四个子命令：`extract`（爬取文档提取契约）、`test`（单次测试）、`batch`（批量探针）、`mine`（缺陷挖掘）
- 修改代码前务必阅读 `THEORETICAL_FRAMEWORK.md` 理解四型缺陷分类法
- Docker 沙箱是测试执行的核心基础设施，所有探针在隔离容器中运行
- DeepSeek API Key 需通过环境变量 `DEEPSEEK_API_KEY` 提供

### Testing Requirements
- `cargo test` 运行单元测试
- 集成测试需要 Docker 环境（`docker-compose.*.yml`）
- 验证脚本（`verify_*.py`、`_verify_*.py`）用于人工复核缺陷

### Common Patterns
- `TargetPlugin` trait：每个向量数据库实现该 trait 以注册到 `TargetRegistry`
- `IndependentReviewer` trait：每个 DB 实现独立审查探针
- `ProbeTemplate` trait：探针模板抽象，减少 per-DB 硬编码
- 契约加载流程：本地 JSON → Knowledge Agent（LLM 从文档/代码仓库提取）
- 缺陷分类：Type-1（非法操作成功）、Type-2（诊断不足）、Type-3（运行时失败）、Type-4（状态/逻辑违规）

## Dependencies

### Internal
- `src/agent/` → `src/contract/`（读取契约约束）
- `src/agent/` → `src/target/`（获取 DB 插件配置）
- `src/agent/` → `src/sandbox/`（沙箱管理）
- `src/agent/` → `src/review/`（独立审查）
- `src/commands.rs` → 所有模块（编排入口）

### External
- tokio 1.52（异步运行时）
- reqwest 0.13（HTTP 客户端）
- serde/serde_json（序列化）
- clap 4.6（CLI 解析）
- chromiumoxide 0.9（浏览器爬取）
- tracing（日志）
