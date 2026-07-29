# TestVDB — Domain Language

TestVDB 自动挖掘向量数据库的合规性缺陷，经多 Agent 辩论与 Docker 沙箱复现后产出可提交的缺陷报告。以下为项目特有领域语言。

## 契约与真相

**Contract**:
从官方文档提取的结构化行为断言，是 Attack/Judge 的依据。
**断言来源，非真相来源**——可能过时或误解（如 weaviate `ef=-1` 的"需正整数"未涵盖 documented sentinel）。
_Avoid_: spec, rule, schema

**Source of Truth**:
实际行为（源码 + 运行时）与维护者权威（PR body / issue / by-design 注释）。
当与 Contract 或 Threat Model 冲突时以此为准，后者被反标错误。
_Avoid_: ground truth, fact

> **真相源层级**：Source of Truth（真相层）> Contract + Threat Model（断言层）。Novelty Gate 纠错层的本质 = 用真相层核验断言层。"contract 反标"以 Dev-Reviewer 的 `root_cause` 分类（contract_misread / hallucination / approximate_by_design）落地，回写 `experience_handoff.json.rejection_patterns` 指导下轮 attack 改进。**直接反标 `structured_contract.json`**（标 stale / patch 修正）作为 future work——见 [ADR-0001](docs/adr/0001-novelty-gate-over-judge-recall.md)（原计划用 `CONTRACT_STALE` 标记 + 回流 threat_model，但因 Phase 0 重生成覆盖 + 无实证需求而暂缓）。

## 缺陷生命周期

**Defect Candidate**:
流水线产出的待判定缺陷假设（端点 + 参数/非法值 + 观察到的契约违规）。
_Avoid_: finding, hit, bug report

**Confirmed Defect**（上位概念）:
通过 4-Judge 辩论的 candidate 的统称。**口头简称**，在精确语境中使用以下两个子级：

**Debate-Confirmed（辩论确认）**:
通过 Stage 2 辩论（evidence + severity + novelty triage + doc）存活的 candidate。进入后续验证管道：VERIFY_LIVE → Reporter → DEFECT_REVIEW → dev-reviewer → Novelty Gate。在管道的任一后续步骤中仍可能被推翻（REFUTED / IRREPRODUCIBLE / FALSE_POSITIVE）。
_Avoid_: confirmed, verified defect

**Gate-Endorsed（闸门背书）**:
通过全部验证层（L1+2 / Reporter 复现 / DEFECT_REVIEW / dev-reviewer）**且** Novelty Gate 判定 NOVEL 的 Debate-Confirmed candidate。可生成 issue 草稿、进入可提交列表。这是流水线唯一承诺"这不是假阳性"的输出级别。
_Avoid_: approved, validated, real bug

## Novelty 与提交

**Novelty Triage（新颖性初筛）**:
judge-novelty Agent 在辩论 Stage 2 中对候选缺陷做初步搜索和标注。使用 5 级标注（new / new_similar / already_reported / known_wontfix / unknown），**不做 kill 决策**——`already_reported` 的候选仍投 `is_defect`，附带 `related_issue_numbers` 传递给 Novelty Gate。
_Avoid_: novelty check, novelty judge（后者暗示最终裁决——初筛不做裁决）

**Novelty Gate（新颖性闸门）**:
Step 9a 中 `scripts/novelty_gate.py` 对 **Debate-Confirmed** candidate 做的独立双层查重（L1 Consumer: threat_model + local corpora / L2 Corrector: GitHub Search API）。使用 6 级分级（NOVEL / KNOWN_OPEN / COVERED_BY_PR / BY_DESIGN / POSSIBLY_FIXED / UNVERIFIED），是**唯一的"是否可提交"权威决策点**。输出 `endorsement=true` 的 candidate 升级为 **Gate-Endorsed**。
_Avoid_: dup-check, novelty judge

> **关键区分**：Triage 是流水线内部的**初筛标注**（收集信息，不 kill），Gate 是流水线末尾的**最终背书**（决定能否提交）。`already_reported` 在 Triage 阶段不 kill candidate——Gate 用更丰富的数据源（threat_model + issue/commit corpus + GitHub API）做精确判定。这解决了 v2.2 中 Triage 误杀的问题：judge-novelty 的 GitHub 搜索不如 Gate 的双层架构全面，过早 kill 会丢失 Gate 能做精确判定（如 COVERED_BY_PR vs KNOWN_OPEN vs BY_DESIGN）的机会。

**可提交背书 (Submittable Endorsement)**:
Novelty Gate 判定 NOVEL，Debate-Confirmed candidate 升级为 Gate-Endorsed，进入可提交列表并生成 issue 草稿。Triage 阶段的 `already_reported` 标注不阻塞此流程——Gate 独立验证。
_Avoid_: approval, pass, green-light

**提交 (Submit)**:
人工把 issue 草稿发到 GitHub 的动作；工具绝不自动执行（见 `AGENTS.md` / `reporter.md`）。
_Avoid_: auto-submit, publish

> **关键区分**：Gate 产出**可提交背书**，不产出**提交许可**。提交始终是人工的——这让 Gate 的 fail-closed 不丢数据（查不清只是不背书，缺陷报告仍生成供人工核）。

**Novelty 分级**（Gate 输出）：

- **NOVEL** — 无已知命中，背书可提交。
- **KNOWN_OPEN** — 精确命中某个 open issue（参数/行为级）。
- **COVERED_BY_PR** — 命中覆盖该参数校验的 PR（open 或 merged）。
- **BY_DESIGN** — 源码或文档显示该"非法值"实际合法，契约前提被推翻。
- **POSSIBLY_FIXED** — 命中已 merged PR，需复验当前版本是否仍可复现。
- **UNVERIFIED** — 查询失败（限流/断网），不背书。

仅 `NOVEL` 进可提交列表；其余内部存档。

## 判定链

**Final Verdict**:
一个 Debate-Confirmed candidate 在流水线结束时的权威判定（聚合全部 Judge + Novelty Gate 的分级与背书），是人工审查的**唯一事实源**；per-round 原始判定仅作溯源。Gate 的判决覆盖 Triage 的初筛标注——即使 Triage 标了 `already_reported`，Gate 仍可判定 NOVEL（Triage 误判）或 COVERED_BY_PR（精确确认）。
_Avoid_: summary, aggregation（后者是 per-round 的，非跨 round 权威）

> 关键约束：Final Verdict 必须**脚本生成、可重跑、带时间戳**，永不手工编辑——否则会与原始判定文件漂移，沦为新的"假事实源"（weaviate v1.38.2 的事后审查正是因为无权威入口、读漏 r2 文件而连环误诊）。

## 建模生态

**Threat Model**:
Phase 0 从目标仓库历史 issue/PR/commit 提炼的缺陷模式、认知盲点与 by-design 行为模型，注入 Attack/Judge Agent 指导挖掘与判定。
它是双刃剑——正确时指导挖掘，错误时（如 weaviate `ef=-1` 被错归为"已确认缺陷 + 推荐攻击目标"，实为 documented sentinel）会**主动驱动假缺陷产出**，故需 Novelty Gate 的纠错层交叉核验、并把纠正回流。
_Avoid_: intelligence（那是本地缓存目录）, model（泛）

## 查重数据源

**Self-archive**:
本工具历史生成的 issue 草稿存档（`issues/`）。Gate 优先查它——自家曾报过的是最强 dup 源（实证：weaviate `dynamicEfMin>Max` 被判 NOVEL，实为作者自家提交的 #11399）。
_Avoid_: local history, cache

## 流水线基础设施 (v2.3.0, ADR-0004~0007)

**Pipeline State** (`scripts/pipeline_state.py`):
流水线状态机深度模块。拥有 `pipeline_state.json` 的全部读写权——11 个 phase 的硬编码 transition map、`advance()`/`mutate()`/`mark_done()` 三个 mutation 方法、WHITELIST 限制的全局状态更新。CLI 版本供 `mine.md` Bash 步骤调用。
_Avoid_: mine state, pipeline config

**Debate Record** (`scripts/debate_record.py`):
`final_verdict.json` 的 schema owner。`FinalVerdict.from_file()` 提供带验证的加载，`DefectVerdict` (16 typed fields) 携带门控分级与背书数据。消费者（reporter / reconstruct_context）导入此模块而非手写 `json.load` + 防御性 `.get()`。
_Avoid_: verdict reader, debate loader

**Check Protocol** (`scripts/checks.py`):
L1 机械检查的形式化接口。`Check` Protocol 定义 `check(candidate, log_path, ctx) → Verdict | None`，`CheckContext` 携带可选依赖（contract / db_url / target）。`verify_live_l1.py` 的 11 个检查全部转为协议类，显式 `ALL_CHECKS` 注册表。
_Avoid_: verification pipeline, filter chain

**Pipeline Utils** (`scripts/_pipeline_utils.py`):
共享脚本基础设施（下划线前缀 = 内部模块）。`setup_encoding()` / `read_json()` / `write_json()` / `debate_log_path()` / `find_log()` / `is_done()` / `touch_done()`——消除了 14 个脚本中的重复样板。
_Avoid_: script helpers, common utils
