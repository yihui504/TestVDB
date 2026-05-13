# Deep Dive Trace: fa-cant-find-new-bugs

## Observed Result
FA 架构重构（AC3+AC1+AC2）完成后，E2E 验证显示 FA 自身仍无法发现任何缺陷。唯一发现的 `hnsw_ef=0` IllegalSuccess 是安全网（hardcoded probe）找到的，不是 LLM 智能探索的结果。架构改进解决了代码组织问题，但未触及核心认知能力问题。

## Ranked Hypotheses
| Rank | Hypothesis | Confidence | Evidence Strength | Why it leads |
|------|-----------|------------|-------------------|--------------|
| 1 | 架构假设根本性错配 | High | Strong | 系统只能发现"单参数边界校验缺失"类缺陷，对竞态/资源泄漏/状态不一致等深层 bug 架构上不支持 |
| 2 | 信息不对称 | High | Strong | FA 只有 2 个工具（execute_test_script + submit_mre），contract 的 violation_examples/min/max 全空，safety nets 拥有 FA 无法获取的 Qdrant 专属知识 |
| 3 | LLM 认知瓶颈 | Medium-High | Moderate-Strong | 模板化 prompt 将 LLM 锚定在教科书式测试模式，但无法区分是模型能力不足还是 prompt 约束 |

## Evidence Summary by Hypothesis

### Hypothesis 1: 架构假设根本性错配
- **F1 (Strong)**: 分类器完全依赖 LLM 自报 `[DEFECT:...]` 标记，无独立判断能力
- **F2 (Strong)**: 测试执行模型是无状态单次请求-响应，每次沙箱全新创建，无法跨轮次保持状态
- **F3 (Strong)**: Safety Net 全部是人工硬编码的边界值探测（零值/负值），系统真正能发现的 Bug 类型仅限于"单参数边界值校验缺失"
- **F4 (Moderate)**: Prompt 模板化严重，LLM 角色被限定为模板填充器
- **F5 (Moderate)**: 无跨会话知识积累机制，每次运行从零开始
- **F6 (Moderate)**: Independent Review 也是硬编码的固定探测脚本

### Hypothesis 2: 信息不对称
- **F1 (Strong)**: FA 只有 2 个工具，KA 有 5 个（含 clone_repo, read_file, search_code, crawl_url）
- **F2 (Strong)**: FA 唯一输入是 contract JSON，直接嵌入 system prompt 末尾
- **F3 (Strong)**: Contract 的 violation_examples 全空、min/max 全 null、setup_script_template 全 null，均为 FUTURE 占位符
- **F4 (Strong)**: 12 轮限制与信息匮乏形成双重约束
- **F5 (Strong)**: Safety Nets 拥有 FA 完全不知道的 Qdrant 专属知识（API 路径、请求体结构、参数组合）
- **F6 (Moderate)**: 断言追踪基于硬编码的 16 个关键词，列表外的参数不被追踪

### Hypothesis 3: LLM 认知瓶颈
- **F1 (Strong)**: System prompt 预定义 5 种测试模板，LLM 被指示从中选择
- **F2 (Strong)**: E2E 结果证实 FA 自身从未发现缺陷
- **F3 (Moderate)**: deepseek-chat + temperature 0.7 属中等偏保守配置
- **F4 (Strong)**: FA 无法读取源码、搜索代码模式、爬取文档

## Evidence Against / Missing Evidence

### Against Hypothesis 1
- **A1 (Moderate)**: LLM 可以在单个脚本内写多步有状态操作（先创建再搜索）
- **A2 (Weak)**: Type-4 StateLogicViolation 分类法定义存在，但实现依赖 LLM 自报
- **A3 (Weak)**: Agentic Loop 提供自适应探索，但仅限同一模板框架内调整

### Against Hypothesis 2
- **A1 (Moderate)**: System prompt 提供了 5 种测试策略模板，部分弥补信息不足
- **A2 (Moderate)**: FA 可以通过执行结果学习（观察服务器实际行为）
- **A3 (Weak)**: 手工 contract 信息密度较高

### Against Hypothesis 3
- **A1 (Moderate)**: ExplorationState 注入提供了自适应策略调整可能性
- **A2 (Weak)**: B2 协议提供强制干预机制
- **A3 (Weak)**: temperature 0.7 提供一定输出多样性

## Per-Lane Critical Unknowns
- **Lane 1 (LLM 认知瓶颈)**: LLM 在无模板约束下的实际行为是什么？瓶颈在模型能力还是 prompt 设计？
- **Lane 2 (信息不对称)**: LLM 的先验知识（训练数据中的 Qdrant 知识）能在多大程度上弥补 contract 的信息不足？
- **Lane 3 (架构假设错配)**: LLM 是否曾自主发现过超出 Safety Net 预设范围的缺陷？

## Rebuttal Round
- **Best rebuttal to leader (H1)**: LLM 可以在单脚本内写多步操作，且 Type-4 分类法理论上覆盖状态不一致
- **Why leader held**: 反驳仅是"理论上可行"，实际 E2E 结果显示从未实现；且单脚本多步操作无法测试跨轮次状态累积、并发竞态等场景

## Convergence / Separation Notes
- **H1 和 H2 收敛**: 两者共同指向"系统只能发现浅层 bug"——H1 说架构不支持深层 bug 发现，H2 说 FA 缺乏发现深层 bug 的信息。根因是同一个：当前架构假设"LLM + contract → bug 发现"，但 contract 只包含浅层约束，架构只支持浅层测试。
- **H3 部分独立**: 即使解决 H1 和 H2（给 FA 更多工具和更丰富的 contract），如果 LLM 本身缺乏创造性推理能力，仍可能无法发现非显而易见的 bug。

## Most Likely Explanation
**系统被设计为"边界值校验探测器"，而非"智能 bug 发现引擎"。** 三条通道的证据收敛于同一结论：

1. 架构层面：无状态单次执行模型 + 依赖 LLM 自报标记 = 只能发现"单参数边界值校验缺失"
2. 信息层面：FA 只有稀疏 contract + 2 个工具 = 无法获取深层知识
3. 认知层面：模板化 prompt + 中等模型 = 只能生成教科书式测试

安全网之所以有效，是因为它包含了人类对 Qdrant 的先验知识和精心设计的测试用例——这恰恰是 FA 缺乏的。

## Critical Unknown
**如果给 FA 提供与 Safety Net 等价的信息（源码访问 + 丰富的 violation_examples + 并发测试能力），LLM 能否自主发现新类型的缺陷？** 这将区分"信息不足"和"认知能力不足"两个子问题。

## Recommended Discriminating Probe
**三因素消融实验**：在 Qdrant v1.18.0 上运行 4 组对照：
1. **基线组**：当前配置（模板 prompt + 稀疏 contract + 2 工具）
2. **信息增强组**：填充 contract 的 violation_examples/min/max + 给 FA 添加 read_file/search_code 工具
3. **认知增强组**：用开放式 prompt 替换模板 prompt + 升级到 deepseek-reasoner
4. **架构增强组**：支持并发测试 + 跨轮次状态保持 + 自动状态一致性检查

每组运行 3 次，记录缺陷发现数量和类型。这将精确定位瓶颈在信息、认知还是架构。
