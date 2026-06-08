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

## PreCompact / PostCompact 上下文保护

### PreCompact
当 Claude Code 发出 PreCompact 信号时（上下文即将压缩），hook 自动触发 `testvdb-pre-compact.js`：
1. 扫描 `results/` 下所有 active 状态的 session（mine_state.json status=running）
2. 保存 mine_state.json + coverage.json + pipeline_state 快照到 `~/.claude/state/testvdb-compact-recovery.json`
3. 创建恢复标记 `~/.claude/state/testvdb-needs-recovery`

### PostCompact
上下文压缩后，hook 自动触发 `testvdb-post-compact.js`：
1. 检查恢复标记是否存在
2. 读取 `testvdb-compact-recovery.json` 中的会话状态
3. 输出恢复提示（session_id、当前轮次、流水线阶段、下一步操作）
4. 主进程根据恢复提示继续执行

**Hook 配置位置**：`~/.claude/settings.json` 的 `PreCompact` 和 `PostCompact` 节。

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
