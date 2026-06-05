<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# vdbfuzz

## Purpose
VDBFuzz 测试生成器集合，包含 9 种确定性的向量数据库测试策略。每种生成器从不同维度构造测试用例：边界值、变异、蜕变、语义、覆盖率、序列、状态、资源组合和并发差异。这些生成器是 TestVDB 自动化缺陷挖掘的核心引擎。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明，注册所有 9 种生成器 |
| `boundary.rs` | 边界值生成器：基于 range_constraint 的 min/max 构造边界测试 |
| `mutation.rs` | 变异生成器：对合法请求参数进行创造性变异（CreativeMutationPrompt） |
| `metamorphic.rs` | 蜕变测试生成器：验证等价输入产生等价输出 |
| `semantic.rs` | 语义测试生成器：验证 API 行为符合文档语义描述 |
| `coverage.rs` | 覆盖率驱动生成器：针对未覆盖的契约约束生成测试 |
| `sequence.rs` | 序列测试生成器：多步操作序列的状态一致性检查 |
| `sequence_gen.rs` | 序列生成器辅助：操作序列的构建和参数化 |
| `state_gen.rs` | 状态生成器：基于 state_constraint 构造状态违规测试 |
| `resource_combo.rs` | 资源组合生成器：测试不同资源组合下的行为 |
| `diff_concurrent.rs` | 并发差异生成器：对比串行和并发执行的结果差异 |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 每种生成器独立实现，互不依赖，可以单独修改
- 添加新生成器：创建新 `.rs` 文件 → 在 `mod.rs` 注册 → 在 `engine.rs` 中调用
- 生成器输入：`StructuredContract`（契约约束）+ `TargetPlugin`（DB 配置）
- 生成器输出：测试脚本（Python）+ 预期行为描述
- 低产出策略自动暂停：约束数 < `strategy_threshold`（默认 100）时跳过 state/meta/seq/res/combo/conc

### Testing Requirements
- 修改生成器后需运行 `mine` 命令验证缺陷发现能力
- 边界生成器是最高效的策略，应优先保证其正确性

### Common Patterns
- 生成器接口：接收契约 + 插件 → 产出测试脚本列表
- 脚本格式：Python 脚本，使用 requests 库调用 DB API
- 断言模式：预期成功/预期失败 → 对比实际结果 → 判定缺陷类型
- 策略优先级：boundary > mutation > semantic > coverage > sequence > state > resource_combo > diff_concurrent > metamorphic

## Dependencies

### Internal
- `contract/schema.rs`（StructuredContract 及约束类型）
- `agent/probe.rs`（ProbeTemplate）
- `target/`（TargetPlugin）

### External
- serde（数据结构序列化）
