---
name: pipeline
description: TestVDB 缺陷挖掘流水线 SOP。当 Orchestrator 编排缺陷挖掘流水线时自动加载。
version: 1.0.0
---

# TestVDB Pipeline Skill

## 触发条件

当 Orchestrator 编排缺陷挖掘流水线时自动加载。非用户手动触发。

## 流水线 SOP

### Phase 1: 知识获取

1. Orchestrator 派 Knowledge Extractor Agent
2. 使用 WebSearch 定位官方文档
3. 使用 WebFetch 抓取 API 参考页面
4. 提取端点、参数、约束
5. 提取 SDK 版本和 Docker tags
6. 输出 `raw_knowledge.md`

### Phase 2: 契约形式化

1. Orchestrator 派 Contract Formalizer Agent
2. 读取 `raw_knowledge.md`
3. 按 JSON Schema 转换为结构化契约
4. 输出 `structured_contract.json`
5. 合同门控检查（覆盖率 ≥ 90%）

### Phase 3: 测试生成

1. Orchestrator 并发派 Attack Trio（boundary + state + semantic）
2. 每个 Agent 独立生成测试脚本
3. 辩论 Stage 1：peer review 投票
4. 通过脚本存入 test_scripts[]

### Phase 4: 沙箱执行

1. Executor 按 DB 选择 Docker 模板
2. 启动容器 → 健康检查
3. 安装依赖 → 执行脚本
4. 收集结果 + 日志
5. 清理容器

### Phase 5: 缺陷判定

1. Judge Trio 并发审查（evidence + novelty + severity）
2. 辩论 Stage 2：投票判定
3. 确认缺陷存入 candidate_defects[]

### Phase 6: 报告生成

1. Reporter 生成 defect-N.md
2. 生成自包含 MRE 脚本
3. 生成 summary.md
4. 保存 session_metadata.json

## 迭代循环

- 每轮结束生成 reflection_context
- 注入下一轮 Attack Agents
- 僵局检测：连续2轮无新缺陷 → 重新搜索文档 → 重新评估候选 → 调整策略
