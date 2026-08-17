# Step 4 缺陷确认重构：证据链双 Agent 架构取代 Judge Quartet + Dev-Reviewer

**Status**: Implemented (2026-08-17；主插件已落地，129 tests 全绿。同步 github.com/yihui504/TestVDB)

## Context

导师 2026-08-17 基于 v3.1 PPT 的反馈确认 Step 4 现架构问题：

1. **四 judges + dev_reviewer 共 5 个 LLM 判定角色**，职责重叠（judge-doc 验文档、judge-evidence 验日志、dev-reviewer 又验一遍文档+源码+复现），且投票/加权聚合机制复杂（stage2 两阶段、doc 门控调节其他 judge 严格度、提交成功率校准等）。
2. **severity 分级无消费方**——导师要求直接删。
3. **judge-novelty 位置错误**——新颖性检查应在"产出提交前"做一次终判，而非每轮对候选做初筛投票。
4. **非 novel 缺陷直接丢弃**，实验上无法统计"发现已被报告 bug"（RQ1 新增列）。
5. **FP 判定未注明证据来源**（文档？源码？两者？），RQ2 人工核查无法量化各来源贡献。

## Decision

### 新架构：三个角色，串行两段 + 提交前终判

```
execution_results[]
      │
      ▼
┌─────────────────────────────────────────────┐
│ evidence-builder（新 agent，按候选并发派发）    │
│  每候选 1 个 builder 实例，并发 fan-out        │
│  （并发上限受 orchestrator 派发槽位约束）       │
│  step1: 收集证据并写入证据链文件                 │
│         （文档验证 + 执行证据审查 + 证据链追溯）│
│  step2: 源码搜证                               │
│         （本地 clone Grep + 调用链追踪）        │
│  产出: evidence_chain/{defect_id}.json        │
│        （每候选独立文件，并发写不冲突）          │
└─────────────────────────────────────────────┘
      │ （全部 builder 完成后收口）
      ▼
┌─────────────────────────────────────────────┐
│ chain-auditor（新 agent，专用证据链检查）       │
│  仅审证据链文件：完备性/一致性/自洽性            │
│  产出: verdict = DEFECT | NOT_DEFECT          │
│         fp_evidence_source = doc|source|both  │
└─────────────────────────────────────────────┘
      │
      ▼ （DEFECT → Reporter；NOT_DEFECT → 归档）
   ……全轮次结束后、提交前……
      │
      ▼
┌─────────────────────────────────────────────┐
│ novelty-check（终判，复用现有 Novelty Gate）    │
│  NON_NOVEL → 归档不删（archived/ 目录）        │
└─────────────────────────────────────────────┘
```

### 1. evidence-builder（合并原 judge-doc + judge-evidence，加证据链追溯）

**不是判定者，是取证者。** 只负责把每个候选缺陷的证据收齐、写实，不做真伪结论。

单 agent 两步（导师指定的 step1/step2）：

- **step1 收集证据并写入证据链文件** = judge-doc + judge-evidence 功能合并 + 证据链追溯：
  - **文档验证**（继承 judge-doc 四层）：source_url 可达性 / 版本匹配（major.minor）/ 内容一致性（预期行为 vs 文档原文，含 sdk_rest_confusion 检测）/ 端点路径精确性（registry 查表 + 联网补充）
  - **执行证据审查**（继承 judge-evidence）：output_*.log 的 reproducibility / isolation / completeness 三维分级（A-D），Type1-4 日志模式识别，多脚本交叉验证，脚本错误检测（script_error 排除）
  - **证据链追溯**（新增，串联两边）：对每个候选把「契约 constraint_id → source_url 文档原文 → 触发脚本 raw 请求/响应 → 日志判定模式」逐环核对，断链处（如 contract 引用了但文档 MISMATCH、或日志 PASSED 但候选声称违规）显式记录 `chain_broken_at`
- **step2 源码搜证**：
  - 本地 clone 自由探索（Grep 关键词 + Read 上下文 + 追调用链）
  - WebFetch 回退时标记 `webfetch_shallow`
  - 平凡解释排除（环境/并发/缓存/参数笔误/by-design）

**产出**：`evidence_chain/{defect_id}.json`，schema 见下方。每条证据必须含来源标注（doc / source / behavior）。

**派发方式：按候选并发派发（2026-08-17 用户拍板）**。evidence-builder 单候选任务重（step1 文档四层验证 + 执行证据审查 + 链追溯，step2 源码搜证需多次 Grep/Read/追调用链），若单 agent 串行处理全部候选，turn 预算与上下文都吃紧、总耗时线性累积。因此：

- **粒度**：1 个 builder 实例处理 1 个候选缺陷（step1 + step2 同 agent 顺序执行——step2 的源码搜证依赖 step1 链追溯发现的断链点，不拆开）。
- **并发**：orchestrator 对 N 个候选并发派发 N 个 builder（受插件并发槽位约束，超出排队）。
- **无写冲突**：产出文件按 defect_id 命名（`evidence_chain/{defect_id}.json`），天然隔离；`.done` 标记沿用现有协议（orchestrator 对每个 builder 检查 `{defect_id}.json.done`）。
- **失败隔离**：单个 builder 失败/超时只影响该候选——按现有保守策略该候选缺链 → chain-auditor 判 NOT_DEFECT（或 NEEDS_MORE_EVIDENCE 补派），不影响其他候选。
- **chain-auditor 收口后置**：全部 builder 完成（或超时）后统一派发，因为它需要完整链集合做跨候选一致性检查。

> 与旧 dev-reviewer SOP 的关系：dev-reviewer 的"主动干净复现 + 前提审计 + 反向证伪"是**审查者自己动手的取证**，本设计 step1 不做——step1 基于已有执行产物（文档 + 日志）做验证与追溯。step1 的证据链追溯保留其"断链处显式记录"的思想。dev-reviewer 的源码接地由 step2 继承、三视角聚合由 chain-auditor 继承。

### 2. chain-auditor（专用 agent——2026-08-17 用户拍板：不用机械脚本）

**判定者，但只读证据链文件。**

- 输入：`evidence_chain/*.json` + structured_contract.json（仅用于核对引证真实性）
- 双盲保留：**禁止读 attack 脚本源码**（防原断言带节奏）
- 职责（对每条链）：
  1. **完备性**：文档验证、执行证据、源码搜证三类是否齐全（源码搜证允许 `not_found_in_source`，但须如实记录）
  2. **一致性**：契约 assertion 原文 vs 文档验证 vs 执行观测 vs 源码逻辑，是否指向同一结论
  3. **自洽性**：证据之间是否矛盾（如源码明确 by-design 但执行观测显示违规——须回 builder 补证）
- 判定规则继承 dev-reviewer 第 6 步三视角聚合（contract/physical 压倒 behavioral），但改为**基于链文件判定**而非自己重跑。
- **FP 判定必须写明 `fp_evidence_source`**：`doc`（仅文档证据足以推翻）/ `source`（仅源码）/ `both`。这就是 RQ2 人工核查的量化基础。

**三视角聚合规则原文保留**（行为优雅不能单独推翻契约或物理违反），作为 chain-auditor 的判定规则，不再作为独立"第 6 步"。

### 3. novelty-check（提交前终判）

- 位置从每轮 stage2 挪到 **Step 9（产出提交前最后一步）**，对全部 DEFECT 候选统一执行。
- 实现复用现有 `scripts/novelty_gate.py`（消费层+纠错层已验证），仅改两点：
  - 输入从 stage2_aggregation 改为 chain-auditor 的 verdict 列表
  - `NON_NOVEL` 处理从"拒绝提交"改为"**移入 `archived/` 目录 + 在汇总中单列**"（供 RQ1"发现已被报告 bug"列统计）

### 删除清单

| 删除项 | 理由 |
|--------|------|
| judge-doc / judge-evidence / judge-severity / judge-novelty 四 agent 文件 | doc+evidence 职能并入 evidence-builder step1，severity 删除，novelty 后置终判 |
| dev-reviewer.md | 源码接地并入 evidence-builder step2，三视角聚合并入 chain-auditor；主动复现/前提审计/反向证伪不继承 |
| stage2 两阶段编排 + doc 门控 + 投票加权聚合 | evidence-builder 并发 fan-out + chain-auditor 串行收口取代，`aggregate_votes.py` 的投票分支废弃 |
| severity 全链路（stage2_severity.json、severity 字段、severity 门控） | 无消费方 |
| 提交成功率校准（v2.1，注入 judge 的门槛调节） | 依赖 judge 体系的 threat_model 注入，随 judges 删除 |
| verify_defects.py 的 severity 校准分支 | 随 severity 删除 |

### 保留清单（有实证价值，不随重构丢失）

| 保留项 | 去向 |
|--------|------|
| 双盲原则（禁读 attack 脚本） | chain-auditor |
| 必须动手复现（禁脑补响应） | evidence-builder step1 |
| 必须引证契约（引到 constraint_id） | evidence-builder step1 |
| 源码接地本地 clone 自由探索 + 浅 fetch 失败模式警示 | evidence-builder step2 |
| 三视角聚合规则（固定聚合公式） | chain-auditor 判定规则 |
| `root_cause_if_fp` 词表 + 回写 experience_handoff | chain-auditor 产出字段 |
| #9255 回归 fixture 自检 | 迁移为 chain-auditor 启动自检 |
| novelty_gate.py 消费层+纠错层 | 提交前终判（见上） |
| 跨轮去重（8e.5） | 保留，输入改为 verdict 列表 |
| .done 标记 + pipeline_state 状态机 | 保留，phase 名更新 |

## evidence_chain/{defect_id}.json schema

```json
{
  "defect_id": "milvus_xxx",
  "endpoint": "entities+search",
  "defect_type": "Type1_InputValidation",
  "built_by": "evidence-builder",
  "steps": {
    "doc_verification": {
      "result": "DOC_VERIFIED | DOC_PARTIAL | DOC_MISMATCH",
      "link_reachability": "PASS | FAIL | PARTIAL",
      "version_match": "PASS | PARTIAL | FAIL",
      "content_consistency": "PASS | PARTIAL | FAIL",
      "endpoint_precision": "PASS | PARTIAL | FAIL",
      "sdk_rest_confusion": false,
      "evidence_source": "doc"
    },
    "execution_evidence": {
      "grade": "A | B | C | D",
      "log_pattern": "FAILED: Type1 | RuntimeFailure | StateViolation | ...",
      "reproducibility": "多脚本稳定触发 | 单脚本 | 间歇 | 环境问题",
      "script_error": false,
      "triggering_scripts": ["test_001.py", "test_014.py"],
      "evidence_source": "behavior"
    },
    "contract_grounding": {
      "constraint_id": "milvus_range_nprobe_001",
      "assertion_text_quoted": "nprobe >= 1 && nprobe <= nlist",
      "api_violates_assertion": true,
      "evidence_source": "doc"
    },
    "chain_trace": {
      "chain_links": ["contract:nprobe_001", "doc:source_url#nprobe", "script:test_001.py", "log:output_B2.log"],
      "chain_broken_at": null,
      "break_detail": null,
      "evidence_source": "doc+behavior"
    },
    "source_grounding": {
      "grep_queries": ["nprobe", "search_params"],
      "files_examined": ["internal/proxy/search.go"],
      "source_excerpt": "...(文件路径+行号+30-50行)",
      "verification_outcome": "validation_absent | validation_present | by_design_in_source | not_found_in_source | webfetch_shallow",
      "evidence_source": "source"
    },
    "mundane_explanation": {
      "excluded": ["env", "concurrency"],
      "surviving": null
    }
  }
}
```

## chain-auditor verdict schema

```json
{
  "defect_id": "milvus_xxx",
  "verdict": "DEFECT | NOT_DEFECT | NEEDS_MORE_EVIDENCE",
  "fp_evidence_source": "doc | source | both | null",
  "perspective_analysis": {
    "contract": {"verdict_A": "CONFIRMED|REFUTED|NEUTRAL"},
    "physical": {"verdict_B": "CONFIRMED|REFUTED|NEUTRAL"},
    "behavioral": {"verdict_C": "CONFIRMED|REFUTED|WEAK_REFUTED"},
    "aggregation_applied": "verdict_A=CONFIRMED → final=DEFECT",
    "final_verdict": "DEFECT"
  },
  "root_cause_if_fp": "contract_misread | ...",
  "rationale": "..."
}
```

`NEEDS_MORE_EVIDENCE`：链内证据矛盾（chain_broken_at 非空、或 doc/source/execution 三方结论不一致）或源码搜证缺失且执行证据非决定性 → 回 evidence-builder 补证一轮（最多 1 次），仍矛盾 → NOT_DEFECT（保守）。

## Consequences

- **轮内判定链路**：Judge Quartet（两阶段 4 agent）+ dev-reviewer（1 agent）= 5 agent 串并行混合 → evidence-builder 按候选并发 fan-out + chain-auditor 串行收口。单候选路径 = 1 取证 + 1 判定；端到端耗时受并发度约束而非候选数线性累积。
- **LLM 调用成本**：每候选从 5 次判定降为 1 次取证 + 1 次判定；builder 并发换来的效率提升以并发 agent 数 × 单 agent turn 预算的峰值上下文为代价，属于时间-峰值资源权衡。
- **RQ2 实证基础**：`fp_evidence_source` 字段直接支撑"靠什么发现 FP（文档/代码，量化贡献）"；`root_cause_if_fp` 支撑"哪个环节引入"。
- **RQ1 新列**：`archived/` 目录 + novelty 终判汇总直接产出"发现已被报告 bug"统计。
- **Phase 2 fixA–fixI 与本管线不再一一对应**——相关数据与实验包单独归档，论文不再保留（2026-08-17 用户拍板）。
- stage2_* 系列文件、debate_logs 投票日志格式废弃；reporter.md 输入改为 verdict 列表。
