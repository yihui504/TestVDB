<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# results

## Purpose
存放测试运行的输出结果，按目标数据库、版本号和时间戳三级目录组织。每次运行生成 defects.json（缺陷列表）和 summary.md（摘要报告）。

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `qdrant/v1.13.0/20260523_094800/` | Qdrant v1.13.0 测试结果（2026-05-23 运行） |
| `weaviate/1.37.4/20260524_*/` | Weaviate 1.37.4 测试结果（2026-05-24 三次运行） |

## For AI Agents

### Working In This Directory
- 结果由 `batch_runner.rs` 和 `mine` 命令自动生成
- 目录结构规范：`results/{target}/{version}/{YYYYMMDD_HHMMSS}/`
- 每次运行包含 `defects.json`（结构化缺陷数据）和 `summary.md`（人类可读摘要）
- 不要手动修改历史结果文件

### Testing Requirements
- 结果格式由 `src/report/generator.rs` 定义
- `defects.json` 遵循 `BatchDefect` 结构

### Common Patterns
- 时间戳目录确保多次运行结果不互相覆盖
- Shadow Mode 对比需要同时引用 Batch 和 Mine 的结果

## Dependencies

### Internal
- `src/report/generator.rs` 生成结果文件
- `src/batch_runner.rs` 触发 Batch 结果输出

### External
- 无
