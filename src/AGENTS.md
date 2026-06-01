<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# src

## Purpose
TestVDB 的全部 Rust 源代码，包含 CLI 入口、命令编排、契约管理、Agent 系统、爬虫、报告生成、沙箱管理、独立审查和目标数据库插件。

## Key Files
| File | Description |
|------|-------------|
| `main.rs` | 程序入口，初始化日志和 CLI，分发到四个子命令（extract/test/batch/mine） |
| `cli.rs` | clap CLI 定义，包含 Extract/Test/Batch/Mine 四个子命令及参数 |
| `commands.rs` | 命令编排逻辑，串联契约加载→Agent 编排→缺陷分类→反馈循环→验证 |
| `contract_loader.rs` | 契约加载流程：本地 JSON → Knowledge Agent 自动提取 → 行为模板增强 |
| `batch_runner.rs` | Batch 模式运行器，启动 Docker 沙箱并执行所有安全网探针 |
| `verification_runner.rs` | 验证管道，对发现的缺陷进行沙箱内独立验证 |
| `feedback_loop.rs` | 反馈循环，将分类结果反馈给 LLM 编排器进行迭代改进 |
| `infra.rs` | Docker 基础设施管理（容器创建/销毁、网络管理、卷清理） |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `agent/` | Agent 系统：LLM 编排器、探针生成、缺陷分类、状态管理（详见 `agent/AGENTS.md`） |
| `contract/` | 契约管理：结构化契约 schema、OpenAPI 解析、约束分析、LLM prompt 生成（详见 `contract/AGENTS.md`） |
| `crawler/` | 文档爬取：Chromium 和 Reqwest 双引擎爬虫、TOC/内容解析（详见 `crawler/AGENTS.md`） |
| `report/` | 报告生成：缺陷报告、LLM 分析、语义门控、验证报告（详见 `report/AGENTS.md`） |
| `review/` | 独立审查：每个 DB 的 IndependentReviewer 实现（详见 `review/AGENTS.md`） |
| `sandbox/` | Docker 沙箱管理：容器生命周期、Python 执行环境（详见 `sandbox/AGENTS.md`） |
| `target/` | 目标数据库插件：TargetPlugin trait + 四库实现（详见 `target/AGENTS.md`） |

## For AI Agents

### Working In This Directory
- 修改入口逻辑在 `main.rs`，添加新子命令需同步修改 `cli.rs` 和 `commands.rs`
- 添加新目标数据库：在 `target/` 下实现 `TargetPlugin` trait，在 `review/` 下实现 `IndependentReviewer` trait
- 契约加载链路：`contract_loader.rs` → `contract/` → `crawler/` → LLM 提取
- 所有模块通过 `mod.rs` 导出公共 API

### Testing Requirements
- `cargo test` 运行所有单元测试
- 契约模块有序列化/反序列化测试（`contract/mod.rs` 中的 `#[cfg(test)]`）

### Common Patterns
- 模块组织：每个子目录有 `mod.rs` 声明子模块和导出公共类型
- 异步编程：所有 I/O 操作使用 `async/await`，运行时为 tokio
- 错误处理：统一使用 `anyhow::Result`
- 日志：使用 `tracing` crate 的 `info!/warn!/error!`

## Dependencies

### Internal
- `commands.rs` 依赖所有子模块
- `agent/` 依赖 `contract/`、`target/`、`sandbox/`、`review/`
- `contract_loader.rs` 依赖 `contract/`、`crawler/`

### External
- tokio（异步运行时）
- anyhow（错误处理）
- clap（CLI）
- tracing（日志）
- serde/serde_json（序列化）
