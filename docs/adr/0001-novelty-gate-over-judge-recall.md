# Novelty 治理：提交前门控作为建模生态的消费者+纠错者

**Status**: Accepted (2026-06-28；v1 两层，回流层推迟 v2)

## Context

weaviate v1.38.2 实证两层问题：

1. `judge-novelty` 跑了 12 次 WebSearch 仍误判 4 NOVEL（含作者自家 #11399）—— judge 召回不可靠。
2. **更深层**：Phase 0 的 `threat_model.json` 把 `ef=-1`（documented sentinel）建模为"已确认缺陷 + 推荐攻击目标 + 高提交成功率"，且未纳入 `by_design_behaviors`——**错误建模主动驱动** by-design 假缺陷 defect-1。

门控若只做"raw 查重旁路"会重蹈 judge 覆辙；若只做"消费建模"会继承建模错误（ef=-1）。

## Decision

提交前 **Novelty Gate** 作为 Phase 0 建模生态的节点，safety net 优先于 judge 召回：

**v1（两层，本次实现）：**
1. **消费层**：精确匹配 `threat_model.json`（`known_ongoing_issues` / `recently_fixed_patterns[].fix_pr` / `by_design_behaviors`）+ 本地全量 `issue_corpus` / `commit_corpus`。确定性、零 API、精度高。
2. **纠错层**：GitHub 增量直查（threat_model 快照后的新 issue/PR）+ **by-design 源码/PR 核验**（不信任建模——ef=-1 靠这层翻案）。

**v2（推迟，待实证）：**
3. ~~回流层~~：原计划"写回 threat_model + 反标 CONTRACT_STALE"，但 `intelligence/` TTL 30 天、`--intel true` 时 threat_model 被 Phase 0 重新生成**覆盖**——回流是临时补丁。持久修正需独立 overlay（`threat_model_overrides.json`）。**无实证前不建**（ponytail：没实证的场景不写代码）。

**关键子决策（grilling + ponytail 审查确认）：**
- `COVERED_BY_PR`(open PR 在途) → 拒绝提交；fail-closed；浅判 PR（title/body 参数匹配）。
- 精度分级：boundary 参数精确→可直接拒绝；state/semantic 行为低精度→降级 `UNVERIFIED` 人工核。
- `BY_DESIGN` 半自动（纠错层提嫌疑+证据，人工拍板）。
- ~~`POSSIBLY_FIXED` 全自动（release date 核对）~~ → **砍**：无实证案例（#11439 是 open 非 merged），降级人工核，等 merged-PR 案例再做。
- ~~`EXECUTION_DEGRADED` 默认标~~ → **砍**：curl 兜底实际行为未核实，先核实再决定。
- 输出 `final_verdict.json`（ADR-0002）为单一事实源；`KNOWLEDGE_DEGRADED` session 的 by-design 嫌疑加权（ADR-0003）。

## Consequences

- v1 两层解决 ef=-1 式建模驱动假缺陷 + novelty 误判。
- **L2 建模缺口**（fix_pr 空 / known_ongoing 漏 / ef=-1 错归）v1 靠消费层精确匹配兜底 + 人工；持久自动修正待 v2 overlay。
- **L3 上游**（bug-shape 识别 by-design / issue→PR 关联 / issue-miner 爬取范围）专项，不阻塞。
- `judge-novelty` 保留；门控与它交叉核验。

## Considered Options (rejected)

- **纯消费者**（信任建模）：继承 ef=-1 错误，defect-1 复发。
- **纯旁路 raw 查重**：重蹈 judge 覆辙，浪费已有建模。
- **回流层原地写 threat_model**：被 Phase 0 周期覆盖，临时补丁，性价比低 → 推迟 v2 overlay。
- **删除 judge-novelty**：否，仍提供优先级信号 + 注入载体。
