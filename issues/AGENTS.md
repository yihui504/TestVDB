<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# issues

## Purpose
存放已发现并记录的向量数据库缺陷报告。每个缺陷以 Markdown 文件记录，包含复现步骤、预期行为、实际行为和证据链。部分缺陷附带 HTML 预览文件和 JSON 格式的 API 响应数据。

## Key Files
| File | Description |
|------|-------------|
| `milvus_duplicate_collection_returns_success.md` | Milvus: 重复创建集合返回成功（Type-1） |
| `milvus_empty_string_params_accepted.md` | Milvus: 空字符串参数被接受（Type-1） |
| `milvus_aliases_list_empty_name.md` | Milvus: 别名列表接受空名称（Type-1） |
| `milvus_nprobe_zero_accepted.md` | Milvus: nprobe=0 被接受（Type-1） |
| `milvus_request_timeout_type_confusion.md` | Milvus: 请求超时类型混淆（Type-1） |
| `milvus_unknown_fields_silently_ignored.md` | Milvus: 未知字段被静默忽略（Type-2） |
| `qdrant_async_upsert_silent_data_loss.md` | Qdrant: 异步 upsert 静默数据丢失（Type-4） |
| `qdrant_filter_returns_non_matching_points.md` | Qdrant: 过滤器返回不匹配的点（Type-4） |
| `qdrant_flat_l2_scoring_ambiguity.md` | Qdrant: Flat L2 评分歧义（Type-4） |
| `qdrant_hnsw_ef_zero_accepted.md` | Qdrant: HNSW ef=0 被接受（Type-1） |
| `qdrant_limit_monotonicity_violation.md` | Qdrant: limit 单调性违规（Type-4） |
| `qdrant_score_threshold_range.md` | Qdrant: score_threshold 范围问题（Type-1） |
| `qdrant_zero_dimension_creates_invalid_collection.md` | Qdrant: 零维度创建无效集合（Type-1） |
| `weaviate_1_ef_min_gt_max.md` | Weaviate: ef 配置 min > max 被接受（Type-1） |
| `weaviate_2_flatcutoff_negative.md` | Weaviate: flatSearchCutoff 接受负值（Type-1） |
| `weaviate_3_replication_negative.md` | Weaviate: 复制因子接受负值（Type-1） |
| `weaviate_4_bq_rescore_negative.md` | Weaviate: BQ rescore 因子接受负值（Type-1） |
| `issue_aliases_body.json` | Milvus 别名问题的 API 响应 JSON |
| `preview_dbname.html` | DB 名称问题预览页面 |
| `preview_timeout.html` | 超时问题预览页面 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 缺陷报告由系统自动生成，格式遵循四型缺陷分类法
- 每个报告包含：标题、缺陷类型、复现步骤、预期行为、实际行为、证据
- 缺陷命名规范：`{target}_{简短描述}.md`
- HTML 预览文件用于可视化展示 API 请求/响应

### Testing Requirements
- 缺陷报告需通过 `verification_runner.rs` 的独立验证
- 验证脚本（项目根目录的 `verify_*.py`）用于人工复核

### Common Patterns
- Type-1（非法操作成功）：违反边界条件但返回成功
- Type-2（诊断不足）：错误信息模糊或不匹配
- Type-3（运行时失败）：合法请求导致崩溃
- Type-4（状态/逻辑违规）：返回成功但内部状态不一致

## Dependencies

### Internal
- `src/report/generator.rs` 生成缺陷报告
- `src/agent/classifier.rs` 进行缺陷分类

### External
- 无
