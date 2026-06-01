<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# target

## Purpose
目标数据库插件模块，定义 `TargetPlugin` trait 和 `TargetRegistry`，并为 Milvus、Qdrant、Weaviate、PGVector 四种向量数据库提供具体实现。每个插件封装了 DB 的 Docker 镜像、端口、探针模板、安全网、审查器等配置。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | TargetPlugin trait 定义 + TargetRegistry 注册表 + SafetyNet 结构 |
| `milvus.rs` | Milvus 插件实现（最成熟，已有 96 缺陷产出） |
| `qdrant.rs` | Qdrant 插件实现 |
| `weaviate.rs` | Weaviate 插件实现 |
| `pgvector.rs` | PGVector 插件实现 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 添加新目标 DB：创建 `{target}.rs`，实现 `TargetPlugin` trait，在 `mod.rs` 的 `new_with_all()` 中注册
- TargetPlugin trait 关键方法：
  - `name()` / `target_image()` / `db_port()`：基础配置
  - `probe_template()`：探针模板（实现 `ProbeTemplate` trait）
  - `safety_nets()`：安全网探针脚本
  - `create_reviewer()`：创建独立审查器
  - `derive_oracle_checks()`：从契约推导 Oracle 检查
  - `default_repo_url()` / `default_docs_url()`：Knowledge Agent 默认 URL
- SafetyNet 结构：`{ name, script, redundant_with_mutation }`
- TargetStyle 枚举：`Qdrant / Milvus / Weaviate / PgVector`

### Testing Requirements
- 新插件需通过 `batch` 命令验证探针可运行
- 修改现有插件需确保回归测试通过

### Common Patterns
- 插件注册：`TargetRegistry::new_with_all()` 预注册四库
- 插件获取：`registry.get("qdrant")` → `Option<&dyn TargetPlugin>`
- 探针模板通过 `probe_template()` 方法获取，实现多态分发

## Dependencies

### Internal
- `agent/probe.rs`（ProbeTemplate trait）
- `agent/oracle.rs`（InvariantCheck）
- `review/`（IndependentReviewer）
- `sandbox/manager.rs`（SidecarSpec）
- `contract/schema.rs`（StructuredContract）

### External
- serde（序列化）
- async_trait（异步 trait）
