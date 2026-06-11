---
name: orchestrator-lifecycle
description: Orchestrator 生命周期管理 — 错误处理、上下文压缩保护、进度可见性、多 DB 并行。
---

# TestVDB Orchestrator — 生命周期管理

> 被 `orchestrator.md` 引用的辅助规范。定义错误处理策略、上下文压缩保护、进度可见性和多 DB 并行建议。

---

## 错误处理

### 分级策略
| 错误类型 | 重试次数 | 退避策略 | 失败后行为 |
|---------|---------|---------|-----------|
| Docker 启动 | 5 | 10s 递增 | **终止会话** |
| 脚本执行 | 5 | 3s 递增 | 跳过该脚本 |
| 文档抓取 | 5 | 5s 递增 | 跳过该端点 |
| LLM 格式不合法 | 5 | 即时 | 降级为低置信度标记 |

所有错误记录到 error_log.json → session 结束汇总到 session_metadata.json。

---

## 上下文保护 — ScheduleWakeup Loop + Hook 安全网

### 主方案：ScheduleWakeup 跨 Turn Loop

流水线采用 **ScheduleWakeup 驱动的跨 Turn 迭代模型**。每轮挖掘是一个独立 Turn：

1. **Turn 1 (FRESH_START)**：Step 1-7 (setup) + Round 1 (8a→8j) → ScheduleWakeup 触发 Turn 2
2. **Turn N (RESUME)**：`reconstruct_context.py` 从磁盘重建上下文 → Round N (8a→8j) → ScheduleWakeup 或终止
3. **Final Turn**：Step 9-10 (汇总 + 清理)

**每轮开始时重建上下文**：
```bash
python scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```
输出包含：当前 phase、已完成的 phases、本轮关键信息、全局进度、下一步行动。

**状态机驱动**：`pipeline_state.json`（v3 schema）是跨 Turn 的唯一状态源。每个 phase 完成后立即更新，确保断点恢复精确到步骤。

**轮内断点恢复**：如果单 Turn 内（8a→8j 中间）触发压缩：
- `phases_completed` 列表记录了已完成的阶段
- `phase_data` 记录了每个阶段的产出摘要
- Loop Turn 入口自动跳过已完成 phase，从断点继续

### 安全网：PreCompact / PostCompact Hook

Hook 作为**最后手段**保护轮内压缩场景。Loop Turn 入口的 `reconstruct_context.py` 是主恢复机制。

#### PreCompact
`precompact_save.py` 保存 `pipeline_state.json`（含精确断点信息）到 `.checkpoints/`。行为不变。

#### PostCompact
`postcompact_verify.py` 读取 `pipeline_state.json`（v3 schema），输出：
- 当前 phase + 已完成 phases
- 精确的恢复指令（从哪个 phase 继续，跳过哪些）
- 如果 turn_type=loop，提示运行 `reconstruct_context.py` 获取完整上下文

PostCompact 输出被注入为 `<system-reminder>`，压缩后的 agent 可据此继续当前 Turn 执行。

### Phase 状态机

```
ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → DEBATE_S2 → 
REPORTING → DEFECT_REVIEW → STATE_SAVE → 
  ├─ ScheduleWakeup → ROUND_START (下一轮)
  └─ CLEANUP → DONE
```

每个 phase 完成后更新 `pipeline_state.json` 的三个关键字段：
- `phase`: 下一阶段名
- `phases_completed`: 追加当前阶段
- `phase_data.{当前阶段}`: 记录产出摘要

---

## 进度可见性

### stdout 实时日志
每轮开始/结束、缺陷发现时即时输出到 stdout：
```
[Round 1/5] Starting Test Generation...
[Round 1/5] Attack Trio: 3 agents dispatched
[Round 1/5] Debate Stage 1: 12/15 scripts passed (3 rejected)
[Round 1/5] Executor: 12 scripts running in sandboxes...
[Round 1/5] Execution complete: 6 passed, 4 failed, 2 error
[Round 1/5] Debate Stage 2: 2 defects confirmed (DataCorruption×1, StateLogicViolation×1)
[Round 1/5] DEFECT FOUND: DataCorruption in /collections/{name} (confidence=0.92)
```

### mine_state.json
持久化状态文件，随时查看进度。

### Monitors（独立守护进程）
- Docker 崩溃监控：检测容器异常退出，自动触发恢复
- 结果目录监控：检测新缺陷文件生成，触发通知

---

## 多DB并行建议

本 Orchestrator 每次只处理一个 DB。如需同时挖掘多个 DB，用户应开多个终端窗口并行执行：
```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
