# Deep Interview Spec: 下一轮Ralph迭代 — 自动化、Bug产出、验证能力三重提升

## Metadata
- Interview ID: di-triple-pillar-2026-06-01
- Rounds: 7
- Final Ambiguity Score: 11.3%
- Type: brownfield
- Generated: 2026-06-01
- Threshold: 0.2
- Initial Context Summarized: no
- Status: PASSED

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.92 | 0.35 | 0.322 |
| Constraint Clarity | 0.90 | 0.25 | 0.225 |
| Success Criteria | 0.85 | 0.25 | 0.213 |
| Context Clarity | 0.85 | 0.15 | 0.128 |
| **Total Clarity** | | | **0.887** |
| **Ambiguity** | | | **0.113** |

## Goal
以**自动化程度**、**bug真实产出能力**、**验证能力**三大支柱为导向，将TestVDB从半手动研究工具升级为无人值守的自动化缺陷挖掘系统。可进行激进架构改造（结果导向），但必须保持对Milvus/Qdrant/Weaviate/PGVector四个DB的通用支持。

### 三大支柱

1. **自动化程度（自愈+全流程闭环）**
   - 五阶段无人值守管线：合同提取 → 缺陷挖掘 → 假阳性过滤 → Issue生成 → 经验交接与多轮循环
   - 自愈：Docker容器挂了自动重启、LLM API超时自动重试(最多3次)、Mine流程中断从断点恢复、沙箱污染自动检测与重建
   - 底线：整个流程必须实现无人值守运行

2. **Bug真实产出能力**
   - 每轮发现≥1个真正有效的新bug（非假阳性、非by-design）即算成功，宁缺毋滥
   - 每个DB的核心CRUD端点100%覆盖（忽视冷门管理端点）
   - 建立合同质量门控：合同过不了门控不进入Mine阶段
   - 不得使用硬编码或刻意指导的方式强行发现已知bug

3. **验证能力**
   - 独立审查者(IndependentReviewer)对所有缺陷做二次验证（变体测试）
   - 结果可复现：任何人拿到Issue文件+Docker配置，一键复现缺陷
   - 验证能力独立于Mine流程：`cargo run -- verify <issue-id>` 单独命令可用

## Constraints
- **数据资产不可动**：contracts/ 和 issues/ 目录下的文件不可删除或破坏
- **四DB通用支持**：Milvus + Qdrant + Weaviate + PGVector 必须全部支持
- **激进改造允许**：代码可大胆改，但改前做好备份便于回滚
- **结果导向**：只要能发现真实bug且保持通用性，架构变更不受限
- **合同门控**：合同质量不达标时拒绝进入Mine阶段，而非用低质量合同勉强运行
- **通用性优先**：不做per-DB的特殊处理来达成验收目标
- **回归不强制但推荐**：现有444个测试尽量保持通过，但如架构需要可以调整测试

## Non-Goals
- 不做新目标DB的扩展（Elasticsearch/ChromaDB等，可后续迭代）
- 不做Web Dashboard或可视化报告
- 不做CI/CD集成（GitHub Actions等）
- 不做学术产出或论文数据收集

## Acceptance Criteria
- [ ] **AC1（自愈）**: Docker容器故障自动重启 + LLM超时自动重试(≤3次) + 沙箱污染自动检测重建 — 三方全通过
- [ ] **AC2（断点恢复）**: Mine流程中断后可从断点恢复，不从头重跑 — 中断测试验证
- [ ] **AC3（无人值守）**: 一个命令启动四DB完整Mine流程，全程无需人工干预，最终产出Issue文件 — 端到端验证
- [ ] **AC4（合同门控）**: 每个DB的合同通过质量门控后才进入Mine，核心CRUD端点100%覆盖 — 门控日志验证
- [ ] **AC5（Bug产出）**: 四DB中至少2个发现≥1个真实新bug（非假阳性、非by-design），且不使用硬编码引导 — Issue文件验证
- [ ] **AC6（假阳性过滤）**: 假阳性漏网率≤5%，真实缺陷抓取率100% — 人工抽样验证
- [ ] **AC7（独立审查）**: IndependentReviewer对所有缺陷执行二次验证+变体测试 — 审查日志验证
- [ ] **AC8（可复现）**: 每个Issue的MRE脚本可在独立Docker环境一键复现 — 复现测试验证
- [ ] **AC9（verify命令）**: `cargo run -- verify <issue-id>` 独立验证命令可用 — 命令行验证
- [ ] **AC10（经验交接）**: 多轮循环之间传递发现经验，不重复探索相同路径 — 日志对比验证
- [ ] **AC11（通用性）**: 四大DB的Mine流程使用统一代码路径，无per-DB特殊分支绕过门控 — 代码审查验证
- [ ] **AC12（备份安全）**: contracts/ 和 issues/ 目录内容在改造前后完全一致 — diff验证

## Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| 自动化就是"一键运行" | 具体追问五阶段 | 五阶段无人值守管线：合同→挖掘→过滤→Issue→循环交接 |
| 60%合同覆盖够用 | Contrarian模式挑战 | 不够，核心CRUD端点必须100%覆盖，且建立合同质量门控 |
| 验证能力就是复现脚本 | Simplifier模式追问 | 独立审查者+一键复现+独立verify命令，三位一体 |
| 自愈是可选项 | 明确追问D底线 | 必须D：全流程无人值守运行是不可谈判的底线 |
| 激进改造可能破坏现有测试 | 明确约束边界 | 可以改测试，但contracts/issues数据资产不可动 |

## Technical Context

### 现有代码库关键模块
- **orchestrator.rs** (~1700行): LLM编排核心，Mine主循环
- **tools.rs**: LLM工具定义（9个工具）
- **oracle.rs**: 不变性检查，已优化(去重+排序+batch_size=20)
- **commands.rs**: CLI入口，mine/extract/test/batch四个子命令
- **contract_loader.rs**: 合同加载+Knowledge Agent提取
- **review/**: 各DB的IndependentReviewer实现
- **target/**: TargetPlugin trait + 四DB实现
- **verification.rs**: MRE验证+缺陷分类
- **sandbox/**: Docker沙箱管理
- **agent/vdbfuzz/**: 9种确定性生成器

### 已知问题（需激进改造解决的）
- Milvus认证头184处硬编码`root:Milvus`
- orchestrator中4处`match target_style`违反插件化
- PgVector Oracle空脚本
- LLM工具接口过于复杂（6个需嵌套JSON的工具）
- system prompt过长(~2000字符)
- 假阳性过滤依赖简单的门控规则，无系统化过滤

### 现有资产
- contracts/: 四个DB的结构化契约JSON
- issues/: 已发现的缺陷报告(Markdown+JSON)
- 5个已提交的GitHub Issue
- 444个测试用例
- Docker Compose配置（四个DB）

## Ontology (Key Entities)
| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| 自动化管线 | core domain | 五阶段(合同/挖掘/过滤/Issue/循环), 自愈(重启/重试/恢复/清洗), 无人值守 | 自动化管线 orchestrate 缺陷挖掘 |
| Bug产出 | core domain | 真实新bug, 假阳性率≤5%, CRUD 100%覆盖, 合同门控 | Bug产出 depends on 合同质量 |
| 验证能力 | core domain | 独立审查, 一键复现, verify命令 | 验证能力 validates Bug产出 |
| 合同质量门控 | supporting | CRUD端点覆盖, 约束密度, 门控通过/拒绝 | 合同质量门控 gates 缺陷挖掘 |
| 假阳性过滤 | supporting | 漏网率≤5%, 真实缺陷100%抓取 | 假阳性过滤 filters 缺陷挖掘 |
| 经验交接 | supporting | 多轮循环, 不重复探索 | 经验交接 feeds 自动化管线 |
| 四DB支持 | constraint | Milvus, Qdrant, Weaviate, PGVector | 四DB支持 constrains 自动化管线 |
| 数据资产 | constraint | contracts/, issues/ | 数据资产 不可变 |
| 无人值守 | constraint | 零人工干预 | 无人值守 constrains 自动化管线 |

## Ontology Convergence
| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|-------------|-----|---------|--------|----------------|
| 1 | 5 | 5 | - | - | - |
| 2 | 7 | 2 | 2 | 3 | 57% |
| 3 | 9 | 3 | 1 | 5 | 56% |
| 4 | 9 | 1 | 0 | 8 | 89% |
| 5 | 9 | 0 | 0 | 9 | 100% |
| 6 | 9 | 0 | 0 | 9 | 100% |
| 7 | 9 | 0 | 0 | 9 | 100% |

## Interview Transcript

### Round 1
**Q:** 对于下一轮开发迭代，你心里更倾向哪种类型的改进？（6选1）
**A:** 在消除现有技术债之后，以项目自动化程度、bug真实产出能力以及验证能力为导向进行开发，必要时可以进行激进改造
**Ambiguity:** 44.0% (Goal: 0.65, Constraints: 0.40, Criteria: 0.45, Context: 0.80)

### Round 2
**Q:** 激进改造的边界在哪里？哪些是不能动的？
**A:** C不能动(contracts/issues数据资产)，A无所谓但要做好备份方便回滚。B是对的(结果导向)。对数据库的支持要注意，目标是通用且有效的系统。
**Ambiguity:** 33.9% (Goal: 0.70, Constraints: 0.70, Criteria: 0.45, Context: 0.85)

### Round 3
**Q:** "自动化程度"理想的终点是什么样子的？
**A:** 自愈能力、可靠的全流程闭环：深入全面的合同提取 → 智能决策的宽且深缺陷挖掘 → 可靠的假阳性自动过滤 → 有效去重markdown格式Issue文件的生成 → 有效的经验交接与多轮循环
**Ambiguity:** 30.8% (Goal: 0.75, Constraints: 0.75, Criteria: 0.45, Context: 0.85)

### Round 4 (Contrarian模式)
**Q:** 如果每一阶段都不需要"完美"，系统最低限度必须能做什么才算有用？
**A:** 假阳性过滤漏5%假阳性但抓100%真实缺陷可接受。每轮1个真实新bug即可，不要假阳性/by-design。合同提取60%不够好。
**Ambiguity:** 25.5% (Goal: 0.80, Constraints: 0.80, Criteria: 0.55, Context: 0.85)

### Round 5
**Q:** 合同"足够好"的标准是什么？
**A:** 核心CRUD端点100%覆盖。合同驱动的Mine在已知有bug的DB版本上必须发现已知bug（但不能硬编码引导）。建立合同质量门控，过不了门控不进Mine。
**Ambiguity:** 19.9% (Goal: 0.85, Constraints: 0.85, Criteria: 0.65, Context: 0.85)

### Round 6 (Simplifier模式)
**Q:** "验证能力"最核心、去掉系统就不完整的东西是什么？
**A:** ABD — 独立审查者对所有缺陷做二次验证 + 结果可复现(一键复现) + 验证能力独立于Mine流程(`cargo run -- verify`)
**Ambiguity:** 14.3% (Goal: 0.90, Constraints: 0.88, Criteria: 0.78, Context: 0.85)

### Round 7
**Q:** 自愈能力的最低底线是什么？
**A:** 选D — 整个流程必须实现无人值守运行，这是底线
**Ambiguity:** 11.3% (Goal: 0.92, Constraints: 0.90, Criteria: 0.85, Context: 0.85)