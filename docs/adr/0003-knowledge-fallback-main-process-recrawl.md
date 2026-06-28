# Knowledge 采集失败的降级策略：复用旧版本 + 强制标记

**Status**: Accepted (2026-06-28；ponytail 审查后从"主进程重爬"改为"复用+标记")

## Context

glm proxy 下 `knowledge-extractor`（WebSearch subagent）频繁 HTTP 400。现状是**静默复用旧版本**——按 `AGENTS.md` Error Log Conventions 归类为"Agent 内部错误自行消化"，不进 `error_log`。weaviate v1.38.2 复用 1.38.0 knowledge，导致 defect-1 契约过时（`ef=-1` sentinel 被当非法）。`mine_state.json` 的 `error_log` 为空，降级完全不可见。

## Decision

knowledge-extractor 失败时，**复用旧版本 knowledge + 强制标记 `KNOWLEDGE_DEGRADED`**（不主进程重爬）：

- 标记 `KNOWLEDGE_DEGRADED: reused from v{old}, agent failed` 到 `mine_state` + `final_verdict.json`
- 记入 `error_log`（跨边界问题——流水线用过时输入，非 Agent 内部重试）
- Novelty Gate **纠错层**对 `KNOWLEDGE_DEGRADED` session 的 by-design 核验加权（契约过时 → "非法"断言更可能错）

**为什么砍掉重爬（ponytail 审查）**：ef=-1 的真正解药是纠错层（by-design 源码/PR 核验），**不依赖 knowledge 是否当版本**；重爬跳过 Crawl4AI + 版本验证，质量未必更好，且每次 400 都重爬是持续开销。纠错层本就要建，复用+标记更 lazy 且够用。重爬作为"标记后仍系统性出错"的 v2。

## Consequences

- 契约可能过时，但纠错层兜底 by-design（ef=-1 式）。
- 降级可见（标记 + `error_log`）。
- 零额外爬取开销。

## Considered Options (rejected)

- **主进程重爬（原方案 B）**：额外开销 + 跳过专业策略，纠错层已兜底，性价比低。
- **阻断**：glm proxy 下 mining 频繁不可用。
