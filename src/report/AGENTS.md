<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# report

## Purpose
报告生成模块，负责将测试结果转化为人类可读的缺陷报告。包含报告生成器、LLM 分析（缺陷描述润色）、语义门控（过滤低质量/幻觉缺陷）和验证报告。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明 |
| `generator.rs` | 报告生成器：将 defects 数据转化为 Markdown 缺陷报告 |
| `llm_analysis.rs` | LLM 分析：使用 DeepSeek 润色缺陷描述、补充复现步骤 |
| `semantic_gate.rs` | 语义门控：过滤 LLM 幻觉产生的假缺陷，确保报告质量 |
| `verification.rs` | 验证报告：生成独立验证结果的可读报告 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- `semantic_gate.rs` 是质量控制的关键，防止幻觉缺陷进入最终报告
- `generator.rs` 生成 Markdown 格式报告，修改报告模板在这里
- `llm_analysis.rs` 调用 DeepSeek API 进行缺陷描述润色

### Testing Requirements
- 修改报告格式后需检查生成的 Markdown 可读性
- 语义门控修改需确保不误杀真实缺陷

### Common Patterns
- 报告流程：defects.json → generator → Markdown 报告
- 质量控制：defects → semantic_gate → filtered defects → report

## Dependencies

### Internal
- `agent/classifier.rs`（缺陷分类数据）
- `agent/llm.rs`（LLM 客户端）

### External
- serde_json（数据序列化）
- chrono（时间戳）
