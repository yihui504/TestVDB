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
2. 优先使用 Crawl4AI 本地 Docker 服务抓取文档（`python scripts/crawl_fetch.py`）
3. 降级使用 WebSearch + WebFetch
4. 提取端点、参数、约束
5. 提取 SDK 版本和 Docker tags
6. 输出 `raw_knowledge.md`

### Phase 2: 契约形式化

1. Orchestrator 派 Contract Formalizer Agent
2. 读取 `raw_knowledge.md`
3. 按 JSON Schema 转换为结构化契约
4. 生成 endpoint_registry（含 source_url, doc_version, doc_quote）
5. 输出 `structured_contract.json`
6. 合同门控检查（核心 CRUD 端点覆盖率 ≥ 90%）

### Phase 3: 测试生成 (v2.0 Fan-Out)

1. Orchestrator 并发派 Attack Trio（boundary + state + semantic）
2. **v2.0 Fan-Out**：每个 Agent 使用 3 种 focus_profile 各派发一次（共 9 并发）
   - priority_first: 优先高严重性约束
   - coverage_gap: 优先低覆盖率端点
   - rejection_pattern: 绕过已知驳回模式
3. 每个 Agent 独立生成测试脚本（最多 30 个/Agent/profile/轮）
3a. **v2.0 跨会话策略注入**：从 Strategy Registry 查询适用策略 → 注入 Attack Agent prompt
    - 高 confidence (>0.7) 策略作为优先攻击模板
    - 应用 migration_rules 中的 DB 特定适配规则
    - `status=deprecated` 的策略不注入
4. 注入 reflection_context + 跨会话策略（首轮无）
5. **汇聚去重**：3 级去重（endpoint + constraint_id + strategy）
6. 辩论 Stage 1：自动化审查（去重 + 语法验证 + 约束验证 + 跨 Agent 交叉审查）
7. 通过脚本存入 `results/{target}/{version}/{timestamp}/script_*.py`

### Phase 4: 沙箱执行

1. Executor 按 DB 选择 Docker Compose 模板
2. 镜像 tag 预检 → 启动容器 → 健康检查
3. 安装 SDK 依赖 → 在独立执行容器中运行脚本
4. 收集结果（stdout/stderr/exit code/HTTP 响应/容器日志）
5. **容器保持运行**（供后续 Judge 和 Reporter 复用）

### Phase 5: 缺陷判定（4-Judge 两阶段辩论）

**阶段 1：** 先派 judge-doc（文档契约验证，权重调节器）
**阶段 2：** 确认 doc 结果后，并发派其他 3 个 Judge：
- judge-evidence：6 维度证据评分（复现性/隔离性/完整性/类型准确性/源可追溯性）
- judge-novelty（Novelty Triage）：GitHub Issues 搜索初筛，5 级标注（new/new_similar/already_reported/known_wontfix/unknown），仅 known_wontfix 投 not_defect，其余投 is_defect
- judge-severity：4 级严重性分类 + 触发频率 + Workaround 评估

**投票逻辑（加权 AND）：**
- evidence=is_defect AND severity∈{Critical,High,Medium,Low} → 辩论确认 (Debate-Confirmed)
- evidence=not_defect OR severity=trivial → 丢弃
- doc_verification_result 作为权重调节器（DOC_PARTIAL/MISMATCH 时降级但不阻塞确认）

### Phase 6: 报告生成

1. Reporter 执行 Pre-Submit Gate（100% 复现验证）
2. 生成 defect-N.md（含 3-Ring 证据链 + 4 型缺陷分类）
3. 生成自包含 MRE 脚本（不依赖 TestVDB 代码）
4. 生成 summary.md
5. 保存 session_metadata.json

## 迭代循环

- 每轮结束生成 reflection_context（key_learnings + rejection_patterns + high_value_endpoints + exhausted_endpoints）
- 注入下一轮 Attack Agents
- 僵局检测：连续 5 轮无新缺陷 → 重新搜索文档 → 重新评估候选 → 调整策略
- 终止条件：僵局 / 覆盖率 ≥ 95% / max_rounds 达到 / min_defects 达到

## Agent 间通信

- 所有 Agent 通过文件系统通信（structured_contract.json, pipeline_state.json, debate_logs/*.json）
- 采用 `.done` 标记文件确保写入原子性
- Orchestrator 检查 `.done` 文件而非直接检查输出文件

## 容器生命周期

- Executor 启动容器，执行完成后**不清理**
- Judge（evidence）复用运行中容器做复现验证
- Reporter 复用运行中容器做 Pre-Submit Gate
- Orchestrator 在每轮结束/会话结束时统一清理
