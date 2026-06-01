<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# shadow_mode_results

## Purpose
存放 Shadow Mode 对比实验的结果数据。Shadow Mode 同时运行 Batch（手工探针）和 Mine（LLM 编排）两种模式，对比缺陷发现效果，用于验证自动化方法的有效性。

## Key Files
| File | Description |
|------|-------------|
| `shadow_mode_report.md` | Shadow Mode 对比报告（Milvus 96 vs 24 缺陷） |
| `filtered_real_defects.md` | 过滤后的真实缺陷列表 |
| `mine_defects.json` | Mine 模式发现的缺陷 |
| `batch_baseline.json` | Batch 模式基线数据 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- Shadow Mode 结果用于论文实验数据
- Mine 模式通常发现 3-4 倍于 Batch 模式的缺陷
- `filtered_real_defects.md` 是经过人工审核的真实缺陷子集

### Testing Requirements
- 对比数据需确保 Batch 和 Mine 使用相同的 DB 版本和配置

### Common Patterns
- Shadow Mode 命令：`cargo run -- mine --target <name> --version <ver> --shadow --skip-verify`

## Dependencies

### Internal
- `src/batch_runner.rs` 生成 Batch 基线
- `src/commands.rs` 编排 Mine 流程

### External
- 无
