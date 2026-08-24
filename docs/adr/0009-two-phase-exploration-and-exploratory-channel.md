# 挖掘循环重构：两阶段探索调度 + exploratory 候选通道 + vein 移除

**Status**: Implemented (2026-08-24；四步 TDD 落地：42af4d5 删 vein → 5d0e638 confirm_per_round → d4aff11 exploratory 通道 → 650673f 两阶段调度+批量探针。311 tests passed / 1 pre-existing failure（test_runtime_qdrant update_aliases，stash 验证与本次改动无关）。本 ADR 亦为 grilling 七项拍板 D1-D7 的固化文档)

## Context

导师 2026-08-24 基于 v3.4 PPT 的第二轮意见，三条主线：

1. **RQ2 错判为 FP 的 TP 过多（FN7）**——要么改进减少，要么放弃宣称并改 RQ1 yield，
   要么降低证据链强度要求换 0 漏报。关键事实：FN7 = violates 误标族 ×4
   （001/003/004/024，链 violates=false 且无机械信号，**零信号而非低强度，
   降阈值碰不到**）+ by-design 抗辩族 ×3（milvus_012/033、qdrant_018，
   判定基础与 GT 来源的系统错位，词表扩展只解决检测不解决判向）。
2. **端到端重挖掘**：目标 = 原发现版本上用最新实现重新发现 GT bug。障碍 =
   旧 bug 部分来自当时脚本 agent 的自由发挥（无严格策略限定），当前 shape-driven
   策略空间覆盖不到。
3. **VDBFuzz 等时对比**：等时预算下，轮内 defect confirmation 占时且 VDBFuzz 无此
   阶段（公平性）；契约分块耗尽后现行逻辑循环重扫 = 空转（orchestrator Step 8：
   "轮数 > 块数则循环"）。

**统一机制缺口**：三条意见汇聚于"低结构化探索的产出，如何进入一个不被一刀切的
候选体系"。本文档以**一个分层机制**回答三条：strict 判定层零改动 +
exploratory 候选通道 + 挖掘侧两阶段调度（收敛后自由探索）。

## Decision 总览（D1-D7，2026-08-24 grilling 拍板）

| # | 决策 |
|---|------|
| D1 | FN7 组合路线：strict 判定层零改动 + 规则5 扩展改道 exploratory 信号 + violates 族交端到端重挖 |
| D2 | 通道三条件准入（有主张 + 推断性支持证据 + 未达 strict）× 三形态（inference_consistency / competing_explanation / behavioral_anomaly）；violates 旧链排除 |
| D3 | 规则5 扩展作信号供给不定案；012/033/018 走通道（覆盖上界 0.909）；v9 降为 limitations |
| D4 | 删 vein；两阶段调度 + 探索模式横切三 attack agent；沙箱小循环 + 批量探针；run2 作废重跑 |
| D5 | confirmation 产品/实验分离：`confirm_per_round` 配置开关；判定/triage 对称排除在等时预算 T 外 |
| D6 | 对比与 re-discovery 共享：milvus/qdrant/weaviate 各 1 最早发现记录版本 |
| D7 | yield 加法叙事（N₁ strict + N₂ exploratory + N₃ 重挖验证，验收线 = 覆盖原收获） |

## 1. vein 移除

**删除清单**：
- `agents/attack-vein.md` 删除
- `agents/orchestrator.md`：vein 派发段、"vein 不受分块约束"例外条款、
  agent 总数 16→15、README/AGENTS.md 同步
- `commands/` 中 mine 命令的 vein 相关引用
- intelligence 注入面检查：bug-shape → attack agents 的注入通道随 vein 关闭
  （settings `intelligence.inject_to_attack_agents` 本就默认 false，确认无残留路径）

**保留**：`agents/bug-shape-extractor.md`（developer_cognition 是判定层 D 视角
输入，独立价值，不受 vein 删除影响）。

**资产移植映射**：

| vein 机制 | 去向 |
|---|---|
| discover-then-deepen 骨架 | §3 沙箱小循环（orchestrator 编排） |
| finding-feedback loop | 探索模式的信号回喂（同上循环） |
| 对照组验证排除 by-design 默认值 | 算子④（行为一致性对照） |
| 8 类 condition 纵深 | 算子②③的具体化清单（移植到各 agent 探索模式段） |
| bug-shape 引导 endpoint 选择 | **丢弃**（GT 同源注入面）。endpoint 优先级改用契约复杂度/覆盖缺口信号（coverage.json 未覆盖端点优先） |

**删除理由留痕**：GT 注入面最大通道消除（vein 消费 bug-shape = 历史 issue 提取）；
论文 0 提及；作用未被受控证明；4 版作废事故涉 vein；越过沙箱直接 curl 真 DB 的
纪律弱点清除。

## 2. 两阶段调度（orchestrator Step 8 重构）

```
Step 8 挖掘循环
  ├── 阶段一：shape-driven 枚举（现行逻辑，改一处）
  │     契约分块派发照旧（每轮一块 chunks[R-1]）
  │     修改："轮数 > 块数则循环重扫" → "轮数 > 块数 → 触发阶段二切换评估"
  │
  └── 阶段二：自由探索（新增）
        触发条件（满足其一）：
          a. 契约块耗尽（round > len(chunks)）
          b. 连续 2 轮无新缺陷且 coverage 无增长
             （mine_state.consecutive_no_defect_rounds ≥ 2 ∧ Δcoverage = 0）
        进入后不回退；reflection_context 继续注入（探索经验进经验环）
        派发：不分块（全契约面 + OpenAPI 原始面），三 attack agent 轮转承接
        派发 prompt 含：四算子菜单 + 目标信号定义 + 批量探针产出规范
```

**僵局终止保留**：探索阶段连续 K 轮（默认 3）无任何信号命中 → 会话终止
（防无限空转；探索不是永动机）。

**RQ2 判定层零改动声明**：本 ADR 全部改动位于挖掘侧与候选标注层；
`check_chain_grounding.py` / `check_physical_constraints.py` 的 verdict 语义
不变（规则5 仅新增信号输出，见 §5）。71 案已定判定与 0.841/0.881 不受影响。

## 3. 四探索算子（agent 规范"探索模式"段，三 attack agent 共享定义）

| 算子 | 行为 | 靶向形态 |
|---|---|---|
| ① 异常响应追踪 | 非 2xx / 超时 / 字段形态异常 → 触发深挖 | 旧"误打误撞"主力 |
| ② 参数空间组合扰动 | 契约外参数、类型混淆、显式空值、边界交叉 | 未知键（033 型）、显式空（012 型） |
| ③ 状态序列扰动 | 并发/交错/删除后操作 | eventual_consistency、concurrency_race 型 |
| ④ 行为一致性对照 | 同族参数异处置、同参数跨接口 | generalization_shapes / interface_parity 的探索化使用 |

**设计纪律**：算子锚定形态学通用性；旧挖掘会话"自由发挥"日志仅作覆盖校验
（四算子是否覆盖旧行为分布），不作设计输入。GT-free 三件套约束沿用（算子不
消费 intel；引导只用契约 + OpenAPI 面 + 响应信号）。

## 4. 沙箱小循环（批量探针协议）

```
attack agent（探索模式）
  产出：一批探针脚本（N ≤ 8，单次会话批量）
        ↓
docker-executor（沙箱，批量执行）
  输出：per-probe 信号摘要（状态码/异常字段标记/超时标记）
        ↓
orchestrator 信号回喂
  命中目标信号 → 下一批聚焦该 endpoint 深挖（算子内变异邻域）
  未命中 → 算子轮转 / endpoint 轮转（覆盖缺口优先）
        ↓
循环预算：每探索轮 M 批（默认 4）——防单点无限深挖
```

**禁止**：探索模式不给 agent 直接执行权（不复刻 vein 自跑）——全走沙箱，
纪律干净；延迟成本用批量探针压。

## 5. exploratory 候选通道（chain-auditor 输出扩展）

**verdict 保持二值**（DEFECT / NOT_DEFECT，strict 不动）。新增输出字段：

```json
{
  "verdict": "NOT_DEFECT",
  "candidate_class": "exploratory_candidate",
  "exploratory": {
    "form": "inference_consistency",
    "signal": "rule5_approx_match",
    "rationale": "…"
  }
}
```

**candidate_class 判定**：
- `verdict=DEFECT` → `strict_defect`
- `verdict=NOT_DEFECT` 且三条件全满足 → `exploratory_candidate`
  - ① has_claim：链内有明确缺陷主张
  - ② has_inferential_support：三形态之一在链内有可指认证据
     （inference_consistency / competing_explanation / behavioral_anomaly）
  - ③ below_strict：机械 A/B 未定案（灰区路径）
- 其余 → `rejected`
  - **排除项（D2）**：violates=false 且无机械信号且链内无主张的旧链 →
    rejected（零信号≠低强度；此类交端到端重挖，不由通道兜底）

**机械辅助（规则5 扩展，D3 改道）**：`check_physical_constraints.py` 新增
近似形态检测（REJECT_MARK 弱匹配 / 前后对照推断替换形态）→ 输出
`exploratory_signal` 字段（**不参与 verdict**），随机械预跑喂 auditor 作通道
标注提示。词表扩展须测试先行（通用形态 RED 用例先落盘，禁单案措辞回调）。

**升级路径（统一判定阶段执行）**：exploratory_candidate → 定向补证
（builder 对照取证 / MRE 复现强化 / 一致性对照复测）→ 过 strict 门槛则升级
strict_defect，否则维持候选身份归档。

## 6. confirm_per_round 配置开关（D5）

`settings.json` 新增：

```json
"mining": {
  "confirm_per_round": true,
  "_confirm_per_round_comment": "true=产品行为（轮内 confirmation，Step 8f 照常）；false=实验特化（跳过轮内 8f/8f.5，candidates.jsonl 累积 + 跨轮去重照常，会话终止后统一判定）"
}
```

- **false 时**：Step 8f/8f.5 跳过（不派 Reporter），容器保持运行至会话终止；
  会话终止（时间到/轮数到/僵局）后统一判定：evidence-builder + chain-auditor
  批量（复用 RQ2 验证过的机械预跑 + SOP 管线）+ novelty 终判 + Reporter。
- **默认 true**：产品行为不变。
- **同插件不 fork**（纪律：插件实现即规范；配置项进实验 checklist 记录）。

**等时口径（论文）**：预算 T = 纯挖掘时间；判定/triage 对称排除在 T 外
（TestVDB 判定层 / VDBFuzz crash 去重最小化同规则）；"发现的 bug" 两边对称
定义 = 离线判定后确认的 defect / triage 后 unique crash。

## 实现/实验边界（2026-08-24 用户原则：实现归实现，实验归实验）

- **实现侧（本 ADR 范围，主插件仓库）**：删 vein、两阶段调度、四算子、批量探针、
  exploratory 通道 schema、confirm_per_round 开关——工具能力迭代，TDD 验收。
- **实验侧（testvdb_paper 仓库，不属于本 ADR）**：GT 分型标注（44 bug 标
  crash/logic 型）、对比实验双轴口径、re-discovery 预注册、run2 重跑执行。
- **禁止交叉污染**：GT 分型是对账口径，不得反哺算子设计（算子只锚形态学
  通用性）；实验配置（confirm_per_round=false）只消费实现提供的开关，不改
  实现语义；实验结果好坏不回滚已定案的实现规范（发现缺陷走 ADR 迭代流程）。

## 实施顺序（TDD，每步 RED 先行）

1. **删 vein**（独立；agent 文件 + orchestrator 引用清理 + 文档同步）
2. **confirm_per_round 开关**（小、独立；TestConfirmPerRoundConfig）
3. **exploratory 通道**（auditor 规范 + candidate_class 判定逻辑 +
   规则5 近似信号；TestExploratoryChannel + 扩展 TestMechanicalBConsistencyRules）
4. **两阶段调度 + 四算子 + 批量探针**（最大件；TestExplorationPhaseSwitch +
   TestBatchProbeProtocol）

测试全绿后主插件 commit + cache 同步；**run2（qdrant v1.18.0）作废重跑**，
RQ1 剩余版本全部统一到本 ADR 规范。

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| 探索模式产出不及 vein 自跑闭环（run2 曾 6/8 DEFECT） | 批量探针压延迟；探索预算 M 批可控；run2 重跑的 GT 重新发现率是硬验收指标 |
| 探索阶段空转（无信号命中） | 算子/endpoint 轮转 + 僵局终止（K=3）保留 |
| exploratory 通道垃圾化 | 三条件准入 + 论文双指标（覆盖率与 precision 同报） |
| 规则5 信号扩展的 in-sample 嫌疑 | 信号不定案（verdict 不变）+ 词表形态学通用 + 测试先行 + 论文定位 error-analysis-driven iteration |

## 关联

- 决策过程：本仓 memory `mentor-round2-decisions.md`（D1-D7 完整依据）
- 前序：ADR-0008（判定双 Agent，本 ADR 不改其 verdict 语义）
- 实验侧：run2 重跑 + 15 版统一；re-discovery 与 VDBFuzz 等时对比（版本
  milvus/qdrant/weaviate 各 1 最早发现记录版，共享本 ADR 产出管线）
