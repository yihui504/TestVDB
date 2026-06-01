<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# contract

## Purpose
契约管理模块，定义结构化契约的数据模型（schema）、OpenAPI spec 解析、约束分析器、契约存储和 LLM prompt 生成。将非结构化文档规则转化为机器可执行的测试约束。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明 + 公共函数：契约加载/保存/合并/约束解析 |
| `schema.rs` | 核心数据结构：StructuredContract、TypeConstraint、RangeConstraint、StateConstraint、BehavioralContract 等 |
| `analyzer.rs` | 结果分析器：将沙箱执行结果与契约约束比对，判定缺陷 |
| `openapi.rs` | OpenAPI spec 解析器：从 OpenAPI JSON 提取端点和参数约束 |
| `store.rs` | 契约存储：ContractStore 管理契约的加载、查询和缓存 |
| `prompt.rs` | LLM prompt 生成器：将契约约束转化为 LLM 可理解的测试指令 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- `schema.rs` 是数据模型基础，修改契约结构需从这里开始
- `mod.rs` 中的 `merge_contracts_from_ka()` 处理 Knowledge Agent 产出的多契约合并
- `mod.rs` 中的 `parse_constraints_from_assertions()` 从自然语言断言提取结构化约束
- 添加新约束类型：在 `schema.rs` 定义结构 → 在 `analyzer.rs` 添加分析逻辑 → 在 `prompt.rs` 添加 prompt 模板

### Testing Requirements
- `mod.rs` 包含序列化/反序列化单元测试
- 修改 schema 后需确保所有现有契约 JSON 仍可正确解析

### Common Patterns
- 契约结构层次：StructuredContract → {type_constraints, range_constraints, state_constraints, behavioral_contracts}
- 约束解析：自然语言断言 → `[type]`/`[range]`/`[state]`/`[behavior]` 前缀标记 → 结构化对象
- 契约合并：多个 StructuredContract → 按类型合并约束 → 单一 StructuredContract

## Dependencies

### Internal
- `agent/`（消费契约进行测试生成）
- `contract_loader.rs`（加载和增强契约）

### External
- serde/serde_json（序列化）
- toml（端点注册表解析）
