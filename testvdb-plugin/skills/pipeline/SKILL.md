---
name: pipeline
description: TestVDB 缺陷挖掘流水线 SOP。当 Orchestrator 编排缺陷挖掘流水线时自动加载。
version: 1.1.0
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
5. 合同门控检查（核心 CRUD 端点覆盖率 ≥ 90%）

### Phase 3: 测试生成

1. Orchestrator 并发派 Attack Trio（boundary + state + semantic）
2. 每个 Agent 独立生成测试脚本
3. **Orchestrator 自行执行辩论 Stage 1**：交叉审查 + 去重
4. 通过脚本存入 approved_scripts[]

### Phase 4: 沙箱执行

1. Executor 按 DB 选择 Docker 模板
2. 启动容器 → 健康检查
3. 安装依赖 → 执行脚本
4. 收集结果 + 日志
5. **容器保持运行**（不清理），供后续 Judge 和 Reporter 使用

### Phase 5: 缺陷判定

1. Judge Quartet 分两阶段审查：
   - Phase 1: judge-doc（文档契约验证）
   - Phase 2: evidence + novelty + severity（读取 doc 结果后并发执行）
2. 辩论 Stage 2：投票判定
3. 确认缺陷存入 candidate_defects[]

### Phase 6: 报告生成

1. Reporter 生成 defect-N.md（含 Pre-Submit Gate 复现验证，复用运行中容器）
2. 生成自包含 MRE 脚本
3. 生成 summary.md
4. 保存 session_metadata.json

### Phase 7: 容器清理

1. Orchestrator 统一清理所有 Docker 容器（`docker compose down -v`）
2. 仅在整轮完成后执行，不提前清理

## 迭代循环

- 每轮结束生成 reflection_context
- 注入下一轮 Attack Agents
- 轮次间重启 DB 容器（`docker restart`）以重置状态
- 僵局检测：连续5轮无新缺陷 → 重新搜索文档 → 重新评估候选 → 调整策略
