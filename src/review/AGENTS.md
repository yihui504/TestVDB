<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# review

## Purpose
独立审查模块，为每个目标向量数据库实现 `IndependentReviewer` trait。独立审查探针在沙箱中运行，用于交叉验证 Agent 发现的缺陷，提供第二道确认。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | IndependentReviewer trait 定义 + 模块声明 |
| `milvus.rs` | Milvus 独立审查实现 |
| `qdrant.rs` | Qdrant 独立审查实现 |
| `weaviate.rs` | Weaviate 独立审查实现 |
| `pgvector.rs` | PGVector 独立审查实现 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 添加新 DB 审查：创建 `{target}.rs`，实现 `IndependentReviewer` trait
- trait 方法：`run_probe()`（执行探针）、`summarize_findings()`（总结发现）
- 审查结果类型为 `ReviewResult = serde_json::Value`

### Testing Requirements
- 审查探针需在对应 DB 的 Docker 环境中运行
- 修改审查逻辑后需确保不引入假阳性

### Common Patterns
- IndependentReviewer trait：`async fn run_probe(&self, sandbox: &Sandbox, port: u16) -> Result<ReviewResult>`
- 审查流程：创建沙箱 → 运行探针 → 收集结果 → 总结发现

## Dependencies

### Internal
- `sandbox/manager.rs`（沙箱管理）
- `agent/classifier.rs`（DefectType 分类）

### External
- async_trait（异步 trait）
- serde_json（结果序列化）
