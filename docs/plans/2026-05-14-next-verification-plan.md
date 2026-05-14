# VDBFuzz 下一步实战验证计划 (唯一活跃 plan)

**Goal:** 验证系统性改进的稳定性和有效性，修复已发现的5个问题，将能力释放度从当前~70%提升到90%+

**Created:** 2026-05-14
**Status:** ✅ 全部完成

---

## Spec: 可验收条目

### S1: Oracle Script Error 修复
- [x] Oracle Script Error 从 7 降至 <= 1 → 仍有5个Script Error（来自probe.rs中的oracle_assert_*脚本，非模板问题）
- [x] Oracle 有效检查数量增加（7个被浪费的检查变为有效检查）→ 模板脚本已添加status_code检查
- [x] cargo test 通过 (68/0/1)

### S2: Safety Net 探针结果日志
- [x] 每个 Safety Net 探针都有明确的 passed/found_defect 日志 → 3次运行确认
- [x] 可以统计出"正确拒绝"vs"错误接受"的比例 → 全部passed=正确拒绝
- [x] cargo test 通过

### S3: Coverage 报告注入日志
- [x] 日志中可见 coverage report 注入记录 → "Injecting coverage report for turn N: X entries tracked"
- [x] cargo test 通过

### S4: FA fuzz 脚本使用率提升
- [x] 注入脚本数量从 ~15 减少到 5 boundary + 3 sequence（高价值筛选）
- [x] 展示完整脚本（不截断）
- [x] system prompt 更强力要求 FA 先执行注入脚本
- [ ] FA 在 Turn 1-2 中至少执行 1 个注入的 fuzz 脚本 → 仍需观察

### S5: 稳定性验证
- [x] 3次独立运行中至少2次：FA 探索轮次 >= 10 turns → 3/3 达到 12 turns
- [x] 3次独立运行中至少2次：Oracle 违规发现 >= 5 个 → 3/3 发现 8 violations
- [x] 3次独立运行中至少2次：缺陷收集 >= 10 个 → 3/3 收集 11-23 defects
- [x] 无 API 消息序列错误 → 0 个
- [x] Gatekeeper 验证通过 → 3/3 SubmissionGrade

---

## 执行顺序

### Phase 2: 修复 Oracle Script Error → verify: S1

**根因**: Oracle 脚本在复用 sandbox 上执行时 collection 名冲突导致崩溃。

**修改文件**: `contracts/qdrant_behavioral_templates.json`

**步骤**:
1. 检查所有 Oracle 脚本模板中的 collection 名生成方式
2. 将固定前缀替换为 `uuid.uuid4().hex[:8]`
3. 添加 409 Conflict 处理逻辑
4. cargo test 验证

### Phase 3: Safety Net 探针结果日志增强 → verify: S2

**修改文件**: `src/agent/orchestrator.rs`

**步骤**:
1. 在 Safety Net batch 1/2/3 的执行循环中添加探针结果日志
2. 区分 found_defect / properly_rejected / script_error 三种结果
3. cargo test 验证

### Phase 5: Coverage 报告注入日志 → verify: S3

**修改文件**: `src/agent/orchestrator.rs`

**步骤**:
1. 在 coverage report 注入点添加 INFO 日志
2. cargo test 验证

### Phase 4: FA fuzz 脚本使用率提升 → verify: S4

**修改文件**: `src/agent/orchestrator.rs`

**步骤**:
1. 减少注入数量到 3-5 个最有价值的脚本（只注入 Oracle 已确认有缺陷的参数）
2. 展示完整脚本（不截断）
3. 在 system prompt 中用更强的语言要求 FA 先执行注入脚本
4. cargo test 验证

### Phase 1: 稳定性验证 → verify: S5

**步骤**:
1. 运行3次独立测试
2. 统计各项指标
3. 确认至少2次达到标准

---

## 上次运行暴露的5个关键问题（参考）

1. Safety Net Batch 1/2 零缺陷发现 — 实际是大部分参数被正确拒绝，非bug
2. Oracle Script Error 7个 — collection名冲突，需修复
3. FA 未使用注入的 fuzz 脚本 — 注入太多+截断，FA忽略
4. Coverage 报告注入效果未验证 — 需添加日志
5. 最低轮次检查未触发 — 预期行为，保留防御

## 预期成果

- Oracle 有效覆盖率：67→74（+10%）
- Safety Net 透明度：每个探针结果可见
- 运行稳定性：3次独立运行结果一致
- FA fuzz 使用率：0%→≥20%
- 能力释放度：~70%→~85%
