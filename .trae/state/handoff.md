# TestVDB 交接信息

## 最后更新: 2026-05-23

## 项目状态: 🔄 Phase 2（缺陷多样性增强）已收尾，Phase A（Qdrant 全流程验证）为下一优先级

---

## Plan/Spec 唯一存放目录

**`TestVDB/.trae/`** — 所有plan/spec只存放在此目录下

| 文件 | 路径 | 状态 |
|------|------|------|
| Plan（主计划） | `PLAN.md` | ACTIVE — Phase A 为当前优先级 |
| Plan（多样性增强） | `.trae/plans/diversity-enhancement-plan.md` | COMPLETED — Step 1-13 完成，Step 14 降级 |
| Spec | `.trae/specs/deep-interview-llm-orchestrator-v2.md` | COMPLETED |
| Handoff | `.trae/state/handoff.md` | 本文件 |

---

## 项目整体进展

### Phase 0: Milvus Shadow Mode ✅ 完成
- Milvus v2.6.16 Shadow Mode 验证完成（Mine 96 缺陷 vs Batch 24 缺陷）
- 5 个 GitHub Issue 已提交到 milvus-io/milvus（3 个 Open, 1 个 Closed, 1 个待确认）
- 筛选出 5 个 P0 + 7 个 P1 + 2 个 P2 待提交缺陷

### Phase 2: 缺陷多样性增强 ✅ 收尾（降级完成）
- Step 1-13 代码全部完成
- 3 个新工具实现：execute_stateful_test / execute_concurrent_test / execute_timing_test
- 硬性约束 + Prompt 重设计 + 语义不变量 + turn_hint 注入
- V33-V41 实战验证（9 次运行）：未产出增量 Bug
- **根因**：Milvus v2.6.16 在状态/并发/时序维度上基本正确
- **结论**：工具能力就绪，需换目标系统验证

### Phase A: Qdrant 全流程验证 ⏳ 待启动（下一优先级）
- A1: 多页面递归爬取 — 未开始
- A2: OpenAPI spec 自动发现 — 未开始（Qdrant 已有 qdrant_openapi.json）
- A3: 分阶段 LLM 合同提取 — 未开始
- A4: 合同交叉验证 — 未开始
- A5: Knowledge Agent 默认启用 — 未开始（QdrantPlugin 已实现 default_repo_url/default_docs_url）
- A6: Qdrant Shadow Mode — 已尝试一次，0 缺陷（合同不足导致策略跳过）
- A7: 探针模板抽象化 — 未开始

### Phase B: Weaviate 探针修复 ❌ 未开始
### Phase C: PGVector 基础覆盖 ❌ 未开始
### Phase D: 横向对比与论文数据 ❌ 未开始

---

## 代码架构总览

### 模块结构（src/）
| 模块 | 文件数 | 功能 | 成熟度 |
|------|--------|------|--------|
| agent/ | 14 | LLM 编排器、执行器、分类器、Oracle、探针、vdbfuzz | ★★★★ |
| contract/ | 5 | 合同加载/解析/Schema/Store/Prompt 生成 | ★★★★ |
| crawler/ | 2 | Reqwest + Chromium 双引擎爬虫 | ★★★ |
| target/ | 5 | 四库 TargetPlugin 适配层 | ★★★ |
| review/ | 5 | 独立审查器（Milvus/Qdrant/Weaviate/PGVector） | ★★★ |
| report/ | 3 | 报告生成/LLM 分析/验证 | ★★★ |
| sandbox/ | 2 | Docker 沙箱管理 | ★★★★ |
| vdbfuzz/ | 10 | 9 种确定性测试生成器 | ★★★★ |

### CLI 命令
| 命令 | 功能 | 状态 |
|------|------|------|
| `extract` | 爬取文档 → 提取合同 | 基本可用，仅爬单页 |
| `test` | 单次 LLM 编排测试 | 可用 |
| `batch` | 批量 SafetyNet 探针 | 可用（Milvus 成熟，Qdrant 可用，Weaviate 11/11 ERROR） |
| `mine` | 合同驱动缺陷挖掘 + 反馈循环 + LLM 编排 | 可用 |

### TargetPlugin 适配层成熟度
| 目标 | SafetyNet 数 | 探针通过率 | 合同质量 | 问题 |
|------|-------------|-----------|---------|------|
| Milvus | 65+ | 79.5% | ★★★★★ (5455行 OpenAPI) | 成熟 |
| Qdrant | 40+ | 未知 | ★★ (合同不足，策略被跳过) | 需要合同扩展 |
| Weaviate | ~11 | 0% (全 ERROR) | ★ (39行) | API 路径/认证不匹配 |
| PGVector | ~10 | 未知 | ★ (36行) | 无 OpenAPI，需 SQL 路径 |

---

## Qdrant 首次 Mine 运行结果

**运行时间**: 2026-05-23 09:48
**结果**: 0 缺陷（所有策略 = 0）
**根因**: 合同约束数 < strategy_threshold(100)，state/meta/seq/res/combo/conc 策略被自动跳过
**待解决**: 需要先完成 A1-A3（爬取+合同提取），扩展合同后再跑 Mine

---

## 已提交 GitHub Issue 跟踪

| Issue | 标题 | 状态 | 优先级 |
|-------|------|------|--------|
| #49823 | nprobe=0 被接受 | Open, triage/accepted, milestone 2.6.18 | P1 |
| #49824 | 重复集合名返回成功 | Closed by author | P0 |
| #49844 | filter=null/missing 被接受 | Open, triage/accepted, milestone 3.0 | P1 |
| #49889 | dbName="" 被接受 | Open | P1 |
| #49890 | Request-Timeout 接受非integer | Open | P1 |

### 待提交缺陷（filtered_real_defects.md）
- P0: 5 个（32768维OOM、REST/SDK不一致、重复ID count=-1、create-drop-create维度丢失、超大维度无上限）
- P1: 7 个（nprobe=-1、负数TTL、超大插入、未知参数静默接受、空collectionName等）
- P2: 2 个（空ID数组、NaN/Inf向量）

---

## 核心瓶颈（来自 PLAN.md）

| 链路 | 评分 | 根因 |
|------|------|------|
| 爬取→合同 | 3/10 | 只爬 TOC 第一页；无 JS 渲染；严重依赖预写资产 |
| LLM 合同提取 | 5/10 | 单次调用；无验证回路；Knowledge Agent 很少触发 |
| Harness 工程 | 6/10 | Docker CLI 依赖；无缓存；多库探针兼容差 |

**关键发现**: Milvus 的 96 个缺陷产出主要依赖预写的 OpenAPI spec（5455 行）和行为模板（1225 行），而非实时爬取+LLM 提取。其他三库缺乏此类资产，整个测试链条从第一环断裂。

---

## 环境信息

### 编译
- Rust edition: 2024
- Cargo build: 成功（0 error, 4 warning 预存）
- 构建目录: C:/t（.cargo/config.toml 配置）

### Docker
- Milvus: docker-compose.milvus.yml (etcd + MinIO + milvus-standalone, 端口 19530)
- Qdrant: docker-compose.qdrant.yml (端口 6333)
- Weaviate: docker-compose.weaviate.yml
- PGVector: docker-compose.pgvector.yml

### DeepSeek API Key
- 位置: C:\Users\11428\Desktop\mftui\deepseekapikey.txt
- 设置: `$env:DEEPSEEK_API_KEY=(Get-Content C:\Users\11428\Desktop\mftui\deepseekapikey.txt -Raw).Trim()`

---

## 下一步行动（优先级排序）

1. **A1: 多页面递归爬取** — 修改 contract_loader.rs 的 run_extract()，实现 BFS 递归爬取
2. **A2: OpenAPI 自动发现** — 爬取后自动检测 /openapi.json 等路径
3. **A3: 分阶段 LLM 合同提取** — 两阶段调用提高准确率
4. **A6: Qdrant Shadow Mode** — 合同扩展后重跑 Mine + Batch
5. **A7: 探针模板抽象化** — ProbeTemplate trait 统一探针逻辑

---

## Milvus 已知并发/竞态 Bug（参考）
- #47913: Flush 后数据 3 分钟不可见（v2.6.11）
- #47635: Load 后 Search 失败（v2.3.x）
- #42723: 并发读写 Panic（v2.6.x）
- #41993: 异步加载死锁（v2.6.0）
- #44078: QueryNode SIGSEGV（v2.6.0→master）
- #44797: Eviction+并发崩溃（v2.6.4）
