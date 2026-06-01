<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# contracts

## Purpose
存放各目标向量数据库的结构化契约文件（JSON 格式），包括从文档提取的约束、OpenAPI spec、爬取的原始页面内容以及行为模板。这些文件是测试生成器的输入，决定了缺陷挖掘的覆盖范围。

## Key Files
| File | Description |
|------|-------------|
| `milvus_contract.json` | Milvus 结构化契约（含类型/范围/状态/行为约束） |
| `milvus_openapi.json` | Milvus OpenAPI spec（5455 行，覆盖完整 API） |
| `milvus_behavioral_templates.json` | Milvus 行为模板（1225 行，含状态一致性/语义正确性等检查） |
| `qdrant_contract.json` | Qdrant 结构化契约 |
| `qdrant_openapi.json` | Qdrant OpenAPI spec |
| `qdrant_behavioral_templates.json` | Qdrant 行为模板 |
| `qdrant_crawled_pages.json` | Qdrant 文档爬取结果（Markdown 页面集合） |
| `weaviate_contract.json` | Weaviate 结构化契约 |
| `pgvector_contract.json` | PGVector 结构化契约 |
| `test*_crawled_pages.json` | 测试用爬取结果 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 契约文件由 `extract` 命令自动生成，通常不需要手动编辑
- 契约质量直接影响缺陷挖掘效果：约束越多，测试覆盖越广
- `*_openapi.json` 文件由爬取时自动发现并下载
- `*_behavioral_templates.json` 是预写的高质量行为约束，Milvus 的模板是缺陷产出的主要来源
- 添加新 DB 契约：运行 `cargo run -- extract --target <name> --docs-url <url> --out-dir contracts/`

### Testing Requirements
- 契约 JSON 必须符合 `src/contract/schema.rs` 中定义的 `StructuredContract` 结构
- 可通过 `cargo test` 中的序列化测试验证格式正确性

### Common Patterns
- 契约文件命名规范：`{target}_contract.json`、`{target}_openapi.json`、`{target}_crawled_pages.json`、`{target}_behavioral_templates.json`
- 契约加载优先级：本地文件 → Knowledge Agent 自动生成

## Dependencies

### Internal
- `src/contract/schema.rs` 定义契约数据结构
- `src/contract_loader.rs` 负责加载和增强契约

### External
- 无（纯数据文件）
