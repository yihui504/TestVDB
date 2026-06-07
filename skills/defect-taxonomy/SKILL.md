---
name: defect-taxonomy
description: TestVDB 四型缺陷分类法参考。当 Judge 或 Attack Agent 需要判定缺陷类型时自动加载。
version: 1.0.0
---

# Defect Taxonomy Reference

## 触发条件

Judge Agents（evidence/novelty/severity）审查候选缺陷时自动加载。Attack Agents 生成测试时可用。非用户手动触发。

## 四型缺陷分类法

### Type 1: Illegal Success（非法操作成功）

**定义**: 违反文档约束的输入被数据库接受（返回 200/201 而非 400/422）。

**示例**:
- `limit=-1` 返回 200 OK 而非 400
- 缺失必需参数 `vector` 返回 200 空结果而非错误
- `distance` 使用不支持的度量返回 201 Created

**检查模式**: expect 4xx → got 2xx

### Type 2: Poor Diagnostics（诊断不足）

**定义**: 数据库正确拒绝了错误输入（返回 4xx/5xx），但错误消息不够清晰。

**诊断质量 Rubric** (3分制):
1. 参数名被提及 (1pt)
2. 正确格式/范围被说明 (1pt)
3. 可操作的修复建议 (1pt)

**阈值**: score < 2 = Type-2 缺陷

### Type 3: Runtime Failure（运行时失败）

**定义**: 合法输入导致数据库崩溃、500 错误或异常行为。

**示例**:
- 合法搜索请求返回 500 Internal Server Error
- 特定向量维度导致容器 crash
- 并发操作导致死锁未被正确处理

### Type 4: State/Logic Violation（状态/逻辑违规）

**定义**: API 正确返回（200 OK），但数据状态或语义结果不一致。

**示例**:
- INSERT 3 rows, COUNT returns 2
- DELETE collection, search still returns data
- UPDATE vector, search returns old results
- 排序结果与向量距离不一致

## 分类决策树

```
1. 是合法输入被拒绝？
   ├── 是 → Type 1 反向（Illegal Rejection）
   └── 否 → 2

2. 是非法输入被接受？
   ├── 是 → Type 1（Illegal Success）
   └── 否 → 3

3. 是合法输入导致崩溃/500？
   ├── 是 → Type 3（Runtime Failure）
   └── 否 → 4

4. 错误消息不清晰？
   ├── 是 → Type 2（Poor Diagnostics）
   └── 否 → 5

5. 状态/结果不一致？
   ├── 是 → Type 4（State/Logic Violation）
   └── 否 → 重新分类或非缺陷
```

## 7-Mode AI Failure Checklist (v2.0)

Reporter 在 Pre-Submit Gate 之前运行的自检机制。详见 `scripts/ai_failure_check.py`。

| Mode | 检查内容 | 检测方法 | 触发行为 |
|------|---------|---------|---------|
| M1 | 脚本错误被误判为数据库缺陷 | 检查 execution_summary.txt | 信息性 |
| M2 | 编造文档引用（幻觉 URL） | curl source_url | REJECT |
| M3 | 编造执行结果数据 | 比对 output_*.log | REJECT |
| M4 | 走捷径跳过关键验证 | 检查 .done 标记 | HALT |
| M5 | 脚本 bug 被说成新发现 | 分类一致性检查 | REWIND |
| M6 | 编造方法论 | attack agent 输出一致性 | REJECT |
| M7 | 锁定早期错误假设 | endpoint 反复驳回 | HALT |
