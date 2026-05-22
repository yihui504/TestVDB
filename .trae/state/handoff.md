# TestVDB 交接信息

## 最后更新: 2026-05-21 14:50

## 项目状态: ⚠️ Step 10-13 代码完成，V33-V41 实战验证未产出增量 Bug

---

## Plan/Spec 唯一存放目录

**`TestVDB/.trae/`** — 所有plan/spec只存放在此目录下

| 文件 | 路径 | 状态 |
|------|------|------|
| Plan（当前） | `.trae/plans/diversity-enhancement-plan.md` | IN PROGRESS (v7) — Step 10-13 代码完成，Step 14 待验证 |
| Plan（Step 10 详细） | `.trae/plans/step10-stateful-model-testing.md` | COMPLETED |
| Spec | `.trae/specs/deep-interview-llm-orchestrator-v2.md` | ACTIVE — Phase 2 Deep Interview 完成（模糊度 13.3%） |
| Handoff | `.trae/state/handoff.md` | 本文件 |

---

## 当前任务：缺陷类型多样性增强（14步）

| Step | 内容 | 状态 |
|------|------|------|
| 1-9 | 前期工作 | ✅ 全部完成 |
| **10** | **有状态模型测试（execute_stateful_test）** | **✅ 代码完成 + V31/V32 验证** |
| **11** | **并发竞态测试（execute_concurrent_test）** | **✅ 代码完成** |
| **12** | **时序依赖测试（execute_timing_test）** | **✅ 代码完成** |
| **13** | **Prompt 重设计 + 硬性约束 + 语义不变量 + turn_hint** | **✅ 代码完成** |
| 14 | 实战验证 — 产出增量 Bug | ⚠️ V33-V41 未产出增量 Bug |

---

## V33-V41 实战验证总结（9 次运行）

### 工具使用情况
- `execute_stateful_test`：✅ LLM 在 Turn 1-3 使用
- `execute_concurrent_test`：✅ LLM 在 Turn 4-6 使用（硬性约束生效）
- `execute_timing_test`：❌ 从未被 LLM 使用（Turn 7-8 LLM 选择其他工具或运行提前结束）
- `execute_test_script`：Turn 9+ 可用，LLM 偶尔使用

### 缺陷发现
- 所有缺陷均为参数边界类型（nprobe=0, shardsNum=null, collectionName type, Authorization null）
- 确定性生成器已覆盖这些类型
- **未产出增量 Bug**

### 根因分析
1. **Milvus v2.6.16 基本状态一致性正确** — insert→rowCount, delete→rowCount, upsert→rowCount 都符合预期
2. **LLM（DeepSeek）倾向参数边界测试** — 即使 Prompt 明确禁止，LLM 仍测试 shardsNum=null, Authorization=null 等
3. **timing_test 从未被使用** — 可能因为 Turn 7-8 时 LLM 已被 Safety Net 或收敛检测提前终止
4. **并发测试未触发竞态** — 2 线程 × 5 实体可能不足以触发 Milvus 竞态条件

### Step 13 代码改动清单
1. **硬性约束**（orchestrator.rs）：
   - Turn 1-3: 只能用 execute_stateful_test 或 compare_endpoints
   - Turn 4-6: 只能用 execute_concurrent_test
   - Turn 7-8: 只能用 execute_timing_test
   - Turn 9+: 任意工具，execute_test_script 从 Turn 10 可用
2. **Prompt 重设计**（orchestrator.rs build_system_prompt）：
   - 语义不变量列表（9 条）
   - 优先端点列表（10 个核心向量操作端点）
   - 具体测试场景模板（3 个 Turn 阶段）
   - 已知 Bug 模式提示（5 种）
   - CRITICAL 禁止测试参数边界
3. **turn_hint 注入**（orchestrator.rs）：每轮根据 Turn 编号注入强制提示
4. **timing test 修复**：
   - immediate=true 时 sleep(0)（真正无延迟）
   - Prompt 示例中 immediate 放在 flush 步骤而非 search 步骤
5. **concurrent test Prompt**：repeat=10（增加竞态窗口）

---

## 关键技术决策

1. **Endpoint规范化**：方案A已实施（openapi.rs 1行改动），效果显著
2. **LLM编排器角色**：从"边界值探索"改为"状态序列探索+跨端点语义推理"
3. **新工具集**：execute_stateful_test + execute_concurrent_test + execute_timing_test + compare_endpoints
4. **硬性约束**：代码层面强制 Turn 分配（非仅 Prompt 建议）
5. **语义不变量**：9 条核心不变量（rowCount, flush 可见性, 并发一致性等）
6. **timing test immediate 语义**：immediate=true → sleep(0)，放在 preparatory 步骤（flush/load/delete）

---

## 待解决问题

1. **timing_test 未被 LLM 使用** — 需要更强的引导或不同的 LLM
2. **LLM 倾向参数边界测试** — DeepSeek 对"不要测试参数边界"指令遵守度低
3. **Milvus v2.6.16 状态一致性正确** — 需要换目标系统或增加负载
4. **降级评估**：
   - 工具能力就绪 ✅
   - ≥1 个增量 Bug 类型 ❌（Milvus v2.6.16 在状态交互/并发/时序维度上没有 Bug）
   - 建议：在 Qdrant 或 Milvus 更早版本上验证

---

## 环境信息

### Milvus Docker
- docker-compose: docker-compose.milvus.yml
- 3容器: etcd + MinIO + milvus-standalone
- 端口: 19530
- 正确版本标签: v2.6.16

### 编译状态
- cargo build: 成功（0 error, 4 warning 预存）
- pip 清华镜像源: 已配置

### DeepSeek API Key
- 位置: C:\Users\11428\Desktop\mftui\deepseekapikey.txt
- 设置: `$env:DEEPSEEK_API_KEY=(Get-Content C:\Users\11428\Desktop\mftui\deepseekapikey.txt -Raw).Trim()`

---

## 已提交 Issue 跟踪
- #49823: nprobe=0 被接受 — Open, triage/accepted, milestone 2.6.18
- #49824: 重复集合名返回成功 — Closed by author
- #49844: filter=null/missing 被接受 — Open, triage/accepted, milestone 3.0
- #49889: dbName="" 被接受 — Open
- #49890: Request-Timeout 接受非integer — Open

---

## Milvus 已知并发/竞态 Bug（Phase 2 目标参考）
- #47913: Flush 后数据 3 分钟不可见（v2.6.11）
- #47635: Load 后 Search 失败（v2.3.x）
- #42723: 并发读写 Panic（v2.6.x）
- #41993: 异步加载死锁（v2.6.0）
- #44078: QueryNode SIGSEGV（v2.6.0→master）
- #44797: Eviction+并发崩溃（v2.6.4）
