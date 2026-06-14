# 批次 D · 命令解耦（Command Decoupling）设计 spec

> 来源：deep-interview（3 轮，ambiguity 18.5%，达标）+ brownfield codebase 探索

- **日期**: 2026-06-14
- **批次**: D（架构演进——命令解耦）
- **类型**: brownfield
- **状态**: 设计完成（deep-interview 达标），待 plan + 实现

---

## Goal（一句话）

把 TestVDB 的"情报采集+建模"、"文档提取+契约生成"、"攻击挖掘"三个阶段从单一 `/testvdb:mine` 全流程入口，拆成**三个可独立触发、灵活配置、智能协作的子命令**（`/testvdb:intel`、`/testvdb:contract`、`/testvdb:mine`），并让 mine 在不显式指定时自动判断是否需要先跑前两者。

## 核心决策（来自访谈）

### 决策 1：三命令职责（Round 1-2）
| 命令 | 职责 | 输入 → 输出 | 缓存 |
|------|------|------------|------|
| `/testvdb:contract <db> <version>` | 文档提取 + 契约生成（knowledge-extractor → contract-formalizer） | db+version → `structured_contract.json` | 168h（`knowledge.cache_ttl_hours`） |
| `/testvdb:intel <db> [--max-issues m] [--max-commits n]` | 情报采集 + 威胁建模（issue-miner → bug-shape → threat-modeler） | db → `threat_model.json` | 30 天（`intelligence.cache_ttl_hours`） |
| `/testvdb:mine <db> <version>` | 攻击挖掘（attack-*→docker-executor→judge→reporter），智能消费 intel/contract | db+version → defects | — |

### 决策 2：mine 向后兼容——智能 mine（Round 1，选 C）
mine 既是**兼容入口**（缺 intel/contract 自动生成），又能**纯挖掘**（缓存命中跳过生成）。默认智能，参数可覆盖。

### 决策 3：mine 参数控制（Round 1，用户补充）
- `--intel true|false`：强制启用/禁用情报生成（覆盖自动判断）
- `--contract true|false`：强制启用/禁用契约生成
- 不传 → 走自动判断（决策 4）

### 决策 4：mine 自动判断逻辑（Round 3，选 D——最周全）
mine 不传 `--intel`/`--contract` 时，按以下**全条件**判断是否复用缓存 vs 重新生成：
1. **缓存存在**：`intelligence/<db>/threat_model.json`、`results/<db>/<version>/structured_contract.json` 在
2. **TTL 新鲜**：情报 < 30 天、契约 < 168h（未过期）
3. **有效性校验**：契约通过 `validate_contract`（schema 合法）、情报含完整 threat_model（cognitive_blindspots/attack_surface）
4. **target/version 匹配**：缓存的 target/version 与请求的 `<db> <version>` 一致

**任一条件不满足 → 该阶段重新生成**。全满足 → 纯挖掘（跳过 intel/contract，省时）。

## Constraints

- **向后兼容**：现有 `/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]` 用法保持工作（智能默认 = 旧行为升级版）
- **不动原有记录**：现有 `results/`/`intelligence/`/`strategy_registry/` 保留；新命令复用缓存机制
- **CC 版本**：subagent 派发需 CC 2.1.165（v2.1.166+ proxy regression，见 [[nested-agent-dispatch-limitation]]）
- **命令注册**：`.claude-plugin/plugin.json` 注册新 commands（intel.md/contract.md）
- **参数 vs 自动判断冲突（C 边界，Round 5）**：`--intel/--contract false` 禁用主动生成，但**完全无缓存** → 报错（无法挖掘）；**有但过期/无效** → 用现有 + 警告。区分"我有但想用旧的" vs "压根没有"
- **agent 复用**：intel/contract 命令复用现有 agent（issue-miner/bug-shape/threat-modeler/knowledge-extractor/contract-formalizer），不新建 agent

## Success Criteria（验收）

1. `/testvdb:contract weaviate 1.38.0` 单独跑 → 生成 `structured_contract.json`，**不启动挖掘**（无 attack/docker/judge）——验证 bug #3 那样的契约调试秒级完成
2. `/testvdb:intel weaviate --max-issues 50 --max-commits 20` 单独跑 → 生成 `threat_model.json`，不跑契约/挖掘
3. `/testvdb:mine weaviate 1.38.0`（intel+contract 缓存全有效）→ **纯挖掘**（跳过生成，日志显示 "intel: cached, contract: cached, mining..."）
4. `/testvdb:mine weaviate 1.38.0`（无缓存）→ 自动生成 intel+contract + 挖掘（= 旧行为）
5. `/testvdb:mine weaviate 1.38.0 --contract false`（**C 边界**，Round 5）：完全无契约 → 报错退出（"契约缺失，--contract false 跳过生成；请先 /testvdb:contract"）；**有但过期/无效** → 用现有 + 警告（"契约可能过期，--contract false 跳过刷新"）。`--intel false` 同理（无情报→报错，有但过期→用+警告）
6. `/testvdb:mine weaviate 1.38.0 --contract true` → 强制重新生成契约（绕缓存）

## Non-Goals（不做）

- 不拆 `/testvdb:attack`（单独攻击生成）——过细，YAGNI（mine 内部已够灵活）
- 不做 `/testvdb:repro`（MRE 再现）——reporter-mre 已有
- 不改 agent 定义（复用现有）
- 不重写 mine 全流程（mine.md 重构为"智能消费 + 挖掘编排"，引用 intel/contract 命令的逻辑，非重写）

## 技术上下文（brownfield）

- **当前 `commands/mine.md`（763 行）**：全流程 SOP（Phase 0 情报 → 知识 → 契约 → 攻击 → 执行 → judge → 报告）。批次 D 把它重构为"挖掘编排 + 智能消费 intel/contract"——提取情报/契约阶段到 intel.md/contract.md，mine.md 变薄（顺带解 834 行 orchestrator 问题）
- **现有缓存机制**：mine 已检测 intelligence/structured_contract 缓存（TTL），有则跳过。批次 D 把这逻辑**显式化 + 提取到 intel/contract 命令** + mine 引用
- **agents/**：情报类（issue-miner/bug-shape-extractor/threat-modeler）、契约类（knowledge-extractor/contract-formalizer）、挖掘类（attack-*/docker-executor/judge-*/reporter）——三类边界清晰，正好对应三命令

## Ontology（Key Entities）

| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| contract 命令 | core | db, version, --force | 产出 → 缓存(契约)；被 mine 消费 |
| intel 命令 | core | db, --max-issues, --max-commits | 产出 → 缓存(情报)；被 mine 消费 |
| mine 命令 | core | db, version, --intel, --contract, --max-rounds, --min-defects | 消费 ← 缓存(情报/契约)；D 判断 |
| 缓存 | supporting | TTL, 有效性, target/version match | intel/contract 产出；mine 判断依据 |
| 控制参数 | supporting | --intel/--contract true/false, m/n | 覆盖 mine 自动判断 |

## Ontology 收敛

| Round | Entities | New | Changed | Stable | Stability |
|-------|----------|-----|---------|--------|-----------|
| 1 | 5 | 5 | - | - | N/A |
| 2 | 5 | 0 | 0 | 5 | 100% |
| 3 | 5 | 0 | 0 | 5 | 100% |

3 轮全收敛——domain model 稳定。

## 访谈关键决策摘要

| 假设 | 挑战 | 决议 |
|------|------|------|
| mine 怎么处理（兼容/重构/智能） | 向后兼容是架构分叉点 | C 智能（兼容入口 + 纯挖掘 + 参数） |
| 解耦成功的标志 | 具体使用场景 | 单独契约调试、intel m/n、mine 智能判断 |
| mine "自动判断"逻辑 | 判断条件 | D 全条件（缓存+TTL+有效性+target/version） |

## 实现影响面（供 plan 参考）

1. **新增 `commands/intel.md`**：情报采集 SOP（issue-miner→bug-shape→threat-modeler），参数 --max-issues/--max-commits
2. **新增 `commands/contract.md`**：契约生成 SOP（knowledge-extractor→contract-formalizer），--force 绕缓存
3. **重构 `commands/mine.md`**：提取情报/契约阶段 → 引用 intel/contract 命令；加智能消费逻辑（D 判断）+ 参数；变薄（顺带解 orchestrator 834 行——orchestrator.md 对应瘦化）
4. **`.claude-plugin/plugin.json`**：注册 intel/contract 命令
5. **可能新增脚本**：缓存有效性检测（mine 的 D 判断 ③有效性——复用 validate_contract + intel 完整性检查）
6. **向后兼容**：mine 现有参数（--max-rounds/--min-defects）保留
