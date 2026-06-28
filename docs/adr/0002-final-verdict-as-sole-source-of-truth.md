# 判定链单一事实源：Final Verdict 作为审查唯一入口

**Status**: Accepted (2026-06-28)

## Context

`debate_logs/` 有 12+ 个 per-round 判定文件（`stage2_{doc,evidence,novelty,severity,aggregation}{,_r2}.json`），r1/r2 并存、无跨 round 聚合。weaviate v1.38.2 的事后审查因只读 r1 漏 r2，**连环误诊两处**（"novelty judge 没跑"、"reporter 凭空断言 4 NOVEL"）。产物结构不防呆 = 设计缺陷，不能归咎于审查者粗心。

## Decision

mining 结束（Novelty Gate 运行后）由脚本自动生成 **`final_verdict.json`**：每个 Confirmed Defect 一行，聚合 4-Judge + Gate 的全部分级 + 最终背书 + 证据 URL + judge↔gate discrepancy。它是人工审查的**唯一事实源**；per-round 原始文件保留作溯源，标注"非审查入口"。

**关键约束**：`final_verdict.json` 必须**脚本生成、可重跑、带时间戳**，永不手工编辑——否则它会与原始文件漂移，成为新的"假事实源"，比分散更危险。

**顺带解决 P4**：它取代 `reporter.md` spec 中虚构的 `candidate_digest.json`（weaviate session 该文件不存在），作为 reporter 的真实权威输入。

## Consequences

- 审查从"拼 12 个文件"变为"读 1 个文件 + 按需溯源"。
- `novelty_gate.py` 顺带产出此聚合（它本就要读全部 judge 文件），零边际成本——`novelty_gate.json` 直接长成 `final_verdict.json` 的超集。
- reporter 输入从虚构的 `candidate_digest.json` 切到真实的 `final_verdict.json`（修 P4 spec rot）。
