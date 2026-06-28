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

> **真相源层级**：Source of Truth（真相层）> Contract + Threat Model（断言层）。Novelty Gate 纠错层的本质 = 用真相层核验断言层；v1 由纠错层标记 by-design 嫌疑，v2 回流层再持久反标 Contract 错误（`CONTRACT_STALE`，需 overlay 防 Phase 0 重新生成覆盖）。

## 缺陷生命周期

**Defect Candidate**:
流水线产出的待判定缺陷假设（端点 + 参数/非法值 + 观察到的契约违规）。
_Avoid_: finding, hit, bug report

**Confirmed Defect**:
通过 4-Judge 辩论的 candidate。仍可能是已知的（未必通过 Novelty Gate 背书）。
_Avoid_: real bug, verified defect

## Novelty 与提交

**Novelty Gate**:
生成 issue 草稿前对 Confirmed Defect 做的独立查重环节；输出"是否背书为 NOVEL"。
_Avoid_: dup-check, novelty judge（后者是流水线内部判定，语义不同——judge 决定 candidate 是否值得做，Gate 决定能否背书提交）

**可提交背书 (Submittable Endorsement)**:
Gate 判定 NOVEL，candidate 进入可提交列表并生成 issue 草稿。
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
一个 Confirmed Defect 在流水线结束时的权威判定（聚合全部 Judge + Novelty Gate 的分级与背书），是人工审查的**唯一事实源**；per-round 原始判定仅作溯源。
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
