<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# agent

## Purpose
Agent 系统核心模块，包含 LLM 编排器（FAOrchestrator）、探针生成、缺陷分类器、LLM 客户端、Oracle 断言检查、状态管理、沙箱运行器和 VDBFuzz 测试生成器集合。这是 TestVDB 缺陷挖掘的"大脑"。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明，导出所有子模块 |
| `orchestrator.rs` | FAOrchestrator：LLM 编排器，驱动测试生成→执行→分类→反馈循环 |
| `engine.rs` | 测试引擎，协调生成器和执行器 |
| `executor.rs` | 测试执行器，在沙箱中运行生成的测试脚本 |
| `classifier.rs` | 缺陷分类器，实现四型缺陷分类法（Type-1/2/3/4） |
| `llm.rs` | DeepSeek LLM 客户端，封装 API 调用和 prompt 管理 |
| `oracle.rs` | Oracle 断言检查，从契约推导不变量并验证 |
| `probe.rs` | ProbeTemplate trait 定义，探针模板抽象接口 |
| `probe_milvus.rs` | Milvus 专用探针实现 |
| `probe_milvus_advanced.rs` | Milvus 高级探针（更复杂的测试场景） |
| `sandbox_runner.rs` | 沙箱运行器，管理测试脚本的容器化执行 |
| `state.rs` | Agent 状态管理，跟踪测试进度和发现 |
| `tools.rs` | LLM 工具定义，供编排器调用的函数接口 |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `vdbfuzz/` | 9 种确定性测试生成器集合（详见 `vdbfuzz/AGENTS.md`） |

## For AI Agents

### Working In This Directory
- FAOrchestrator 是核心编排逻辑，修改测试流程从这里入手
- 添加新探针：实现 `ProbeTemplate` trait 或在 `probe_*.rs` 中添加
- 添加新生成器：在 `vdbfuzz/` 下创建新模块并在 `vdbfuzz/mod.rs` 注册
- LLM 交互通过 `llm.rs` 的 `DeepSeekClient`，不要直接调用 reqwest
- 缺陷分类逻辑在 `classifier.rs`，修改分类标准需同步更新 `THEORETICAL_FRAMEWORK.md`

### Testing Requirements
- 修改编排器逻辑后需运行完整 `mine` 流程验证
- 分类器修改需确保四型分类的 MECE 性质不被破坏

### Common Patterns
- 编排流程：生成器产出 → 执行器沙箱运行 → 分类器判断 → 反馈循环
- LLM 调用：prompt 构造 → DeepSeek API → 响应解析 → 脚本提取
- 沙箱执行：脚本写入容器 → Python 执行 → 结果收集 → 日志分析

## Dependencies

### Internal
- `contract/`（读取契约约束和 prompt 模板）
- `target/`（获取 DB 插件配置和探针模板）
- `sandbox/`（沙箱管理）
- `review/`（独立审查）

### External
- DeepSeek API（LLM 推理）
- Docker（沙箱执行环境）
