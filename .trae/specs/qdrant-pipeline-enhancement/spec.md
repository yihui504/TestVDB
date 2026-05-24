# Qdrant 全流程验证与管道增强 Spec

## Why
Qdrant 首次 Mine 运行产出 0 缺陷。表面原因是合同约束数 < strategy_threshold(100)，但根因分析揭示：**合同到探针的映射管道断裂**才是核心瓶颈。`generate_probe()` 只能处理 6 种 EndpointType 的硬编码模板，`ProbeTemplate` trait 已定义但零引用，`classify_endpoint_type()` 的 fallback 导致配置类约束被错误路由。需要先修管道，再灌数据。

## What Changes
- **A7**（最高优先级）: 将 ProbeTemplate trait 改为脚本级（方法返回完整 Python 脚本），新增 `assemble_script()` 辅助函数，修复 classify_endpoint_type() fallback，替换 9 个硬编码调用点
- **A2**: 增强 OpenAPI 自动发现（补充 Qdrant 特有路径），OpenAPI 直取优先于爬取
- **A3**: 实现第三阶段参数遗漏检测（与现有 `augment_contract()` 互补，非重复），含无 OpenAPI 降级策略，目标 ≥30 参数
- **A1**: BFS 爬取验证与调优（降级为补充手段，仅在 A2+A3 不够时启用）
- **A6**: Qdrant Shadow Mode 分步执行（Batch 基线 → 量化 Go/No-Go → Mine）

## Impact
- Affected code: `agent/probe.rs`, `target/mod.rs`, `target/qdrant.rs`, `target/milvus.rs`, `contract_loader.rs`, `agent/vdbfuzz/boundary.rs`, `agent/oracle.rs`, `agent/vdbfuzz/diff_concurrent.rs`, `agent/vdbfuzz/sequence_gen.rs`
- Affected capabilities: 探针生成通用性、合同提取质量、Qdrant 缺陷发现能力

## ADDED Requirements

### Requirement: A7 — ProbeTemplate 脚本级重构与硬编码调用点替换（最高优先级）
系统 SHALL 将 ProbeTemplate trait 改为脚本级（方法返回完整 Python 脚本），新增 `assemble_script()` 辅助函数，替换 9 个硬编码调用点。

#### Scenario: ProbeTemplate 改为脚本级
- **WHEN** ProbeTemplate trait 定义
- **THEN** 每个方法返回完整 Python 脚本 `String`（含 preamble + setup + test + verdict），而非表达式片段
- **AND** 方法签名：`fn search_probe(&self, param: &str, value: &str, label: &str) -> String`
- **AND** QdrantProbeTemplate 和 MilvusProbeTemplate 均实现全部方法
- **AND** 新增 `assemble_script(preamble: &str, setup_lines: &[&str], test_step: &str) -> String` 辅助函数，用于组合公共脚本结构

#### Scenario: assemble_script() 辅助函数
- **WHEN** 需要组装探针脚本
- **THEN** `assemble_script()` 接受 preamble、setup 行列表、test 步骤，输出完整 Python 脚本
- **AND** 输出格式与现有 `search_probe()` 等函数的输出结构一致（import → BASE 定义 → setup → test → verdict）
- **AND** QdrantProbeTemplate 和 MilvusProbeTemplate 的方法内部使用 `assemble_script()` 减少重复

#### Scenario: EndpointType 扩展 Config 变体
- **WHEN** `classify_endpoint_type()` 遇到 `quantization_config`、`optimizer_config`、`wal_config` 等配置参数
- **THEN** 返回 `EndpointType::Config` 而非 fallback 到 `EndpointType::Search`
- **AND** Config 类型使用 `update_config_probe()` 方法生成探针（先创建集合，再 PATCH 修改配置）

#### Scenario: 9 个硬编码调用点增量替换（P0→P1→P2→P3→P4）
- **WHEN** A7 集成执行
- **THEN** 9 个调用点按以下优先级增量替换，每步后 golden output 对比 + `cargo test` 验证：

**P0 — 核心分发器（Qdrant 立刻受益）**
1. `generate_probe()` ([probe.rs:47-60](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/probe.rs#L47-60)) → 内部调用 `QdrantProbeTemplate` 方法
2. `SimpleSafetyNet::to_safety_net()` ([probe.rs:364-372](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/probe.rs#L364-372)) → 内部调用 `generate_probe()`（P0.1 的下游消费者）

**P1 — 根因修复（contract→probe 管道打通）**
3. `OracleCheckDeriver::from_range_constraints()` ([qdrant.rs:1055-1172](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/target/qdrant.rs#L1055-1172)) → 改为调用 `generate_probe()` 或直接使用 ProbeTemplate

**P2 — 合同驱动测试主入口**
4. `boundary.rs::make_case()` ([boundary.rs:197-240](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/vdbfuzz/boundary.rs#L197-240)) → 通过 ProbeTemplate 生成脚本
5. `milvus_generate_probe()` ([boundary.rs:307-324](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/vdbfuzz/boundary.rs#L307-324)) → 统一到 `MilvusProbeTemplate`
6. `qdrant_float_probe()` / `qdrant_missing_required_probe()` / `qdrant_invalid_enum_probe()` ([boundary.rs:326+](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/vdbfuzz/boundary.rs#L326)) → 通过 ProbeTemplate 生成

**P3 — Milvus 适配器增量迁移**
7. `MilvusSimpleSafetyNet` 20+ 适配器方法 ([probe_milvus.rs:1024-1264](file:///c:/Users/11428/Desktop/mftui/TestVDB/src/agent/probe_milvus.rs#L1024-L1264)) → 逐方法内部改用 `MilvusProbeTemplate`，保留方法结构不变

**P4 — 长尾清理**
8. `diff_concurrent.rs` 中的硬编码探针脚本 → 通过 ProbeTemplate 生成
9. `sequence_gen.rs` 中的硬编码探针脚本 → 通过 ProbeTemplate 生成

#### Scenario: Golden Output 对比验证
- **WHEN** 每个替换步骤完成
- **THEN** 新实现输出与旧实现输出逐字节对比（golden output 测试）
- **AND** 对于 P0/P1：输出必须完全相同
- **AND** 对于 P2-P4：输出行为等价（允许格式微调，但 API 路径、参数注入、verdict 逻辑必须一致）

#### Scenario: A7 回滚策略
- **WHEN** 某调用点替换后 cargo test 失败
- **THEN** 保留原硬编码实现作为 fallback（通过 `#[cfg(feature = "legacy_probe")]` 条件编译），不阻塞下游任务
- **AND** 每个 Phase 完成后打 git tag（如 `a7-p0-done`、`a7-p1-done`）以便回滚
- **AND** 记录失败原因，在后续迭代中修复

#### Scenario: TargetPlugin 提供 ProbeTemplate
- **WHEN** `TargetPlugin` trait 新增 `probe_template()` 方法
- **THEN** QdrantPlugin 返回 `QdrantProbeTemplate`，MilvusPlugin 返回 `MilvusProbeTemplate`

#### Scenario: A7 回归验证
- **WHEN** ProbeTemplate 集成完成
- **THEN** `cargo test` 中所有已有测试仍然通过
- **AND** 新增 golden output 对比测试：`QdrantProbeTemplate::search_probe()` 输出与 `search_probe()` 完全相同
- **AND** 新增 golden output 对比测试：`MilvusProbeTemplate::search_probe()` 输出与 `milvus_search_probe()` 完全相同
- **AND** 新增测试：`QdrantProbeTemplate::update_config_probe()` 生成的脚本包含 `PATCH` 方法
- **AND** 新增测试：`classify_endpoint_type("quantization_config", "...")` 返回 `EndpointType::Config`

### Requirement: A2 — OpenAPI 自动发现增强
系统 SHALL 在爬取后自动探测 OpenAPI spec，包括 Qdrant 特有的路径。

#### Scenario: 发现 Qdrant OpenAPI spec
- **WHEN** 执行 extract 命令且本地无 `{target}_openapi.json`
- **THEN** 系统探测标准路径 + Qdrant 特有路径（`api.qdrant.tech` 子域、`/redoc/openapi.json`）
- **AND** 发现有效 OpenAPI JSON 后保存到 `{target}_openapi.json`

#### Scenario: OpenAPI 衍生约束数验证
- **WHEN** OpenAPI spec 成功加载
- **THEN** `OpenApiParser::extract_to_contract_store()` 产出的 type_constraints + range_constraints ≥ 20 条

### Requirement: A3 — 三阶段合同提取（第三阶段为参数遗漏检测，与 augment_contract 互补）
系统 SHALL 采用三阶段提取流程。第三阶段专注于**参数遗漏检测**（比对 LLM 产出参数列表与 OpenAPI 参数列表，补全遗漏参数的约束），与现有 `augment_contract()` 的运行时约束合并互补。

#### Scenario: 三阶段提取产出高质量合同
- **WHEN** 执行 extract 命令
- **THEN** 阶段 1（LLM 参数提取）→ 阶段 2（LLM 约束提取）→ 阶段 3（参数遗漏检测 + 补全）
- **AND** 最终合同 assertions ≥ 30 条
- **AND** type_constraints + range_constraints 总数 ≥ 15 条

#### Scenario: Phase 3 参数遗漏检测（与 augment_contract 的区别）
- **WHEN** 阶段 2 产出合同后
- **THEN** Phase 3 从 OpenAPI spec 提取完整参数列表（权威源）
- **AND** 比对 LLM 产出的参数列表，识别遗漏参数
- **AND** 对遗漏参数，用 OpenAPI spec 的 schema 信息直接补全约束（无需 LLM）
- **AND** 输出补全报告（新增多少参数/约束）
- **AND** **与 `augment_contract()` 的区别**：`augment_contract()` 在运行时合并已有约束文件（幂等操作），Phase 3 在提取时检测参数遗漏（增量操作）。Phase 3 的补全结果会被 `augment_contract()` 进一步合并，两者不冲突

#### Scenario: 无 OpenAPI spec 时的降级策略
- **WHEN** OpenAPI spec 不可用（A2 自动发现失败且本地无文件）
- **THEN** Phase 3 降级为：对 Phase 2 产出的 assertion 做结构化解析验证
- **AND** 解析失败的 assertion 回退到 LLM 重新格式化（单条重试，非全量重跑）
- **AND** 最终合同 assertions ≥ 20 条（降级目标）

#### Scenario: 准确率定义
- **WHEN** 评估合同质量
- **THEN** "准确率"定义为：OpenAPI spec 中存在的参数被合同覆盖的比例（参数级召回率）
- **AND** 验收标准：参数级召回率 ≥ 80%（有 OpenAPI 时）或 ≥ 60%（降级模式时）

### Requirement: A1 — BFS 爬取验证与调优（降级为补充手段）
系统 SHALL 在 A2+A3 仍不够时启用 BFS 爬取作为补充。

#### Scenario: Qdrant 文档爬取达到 20 页
- **WHEN** 执行 `testvdb extract qdrant https://qdrant.tech/documentation/ contracts`
- **THEN** 爬取结果包含 ≥20 个 CrawledPage 条目，每个 markdown 长度 ≥100 字符

#### Scenario: 增量模式合并已有页面
- **WHEN** 已有 `qdrant_crawled_pages.json` 存在
- **THEN** 新爬取页面与已有页面按 URL 去重合并，不丢失已有数据

### Requirement: A6 — Qdrant Shadow Mode（分步执行，量化 Go/No-Go）
系统 SHALL 分步完成 Qdrant Shadow Mode，包含量化 Go/No-Go 决策点。

#### Scenario: A6.1 — Batch 基线确认
- **WHEN** 执行 `testvdb batch qdrant`
- **THEN** ≥20 个 SafetyNet 探针执行，通过率 ≥ 60%

#### Scenario: A6.2 — 量化 Go/No-Go 决策点
- **WHEN** Batch 基线确认完成
- **THEN** Go 判据：探针通过率 ≥ 60% **且** 假阳性率 < 30%
- **AND** 假阳性定义：脚本 stdout 包含 `[DEFECT:` 但 exit code != 1（即标记为缺陷但实际执行异常）的探针数 / 总 DEFECT 标记数
- **AND** No-Go 判据：通过率 < 60% 或假阳性率 ≥ 30% → 回退检查探针管道

#### Scenario: A6.3 — Mine 运行
- **WHEN** Go/No-Go 通过后执行 `testvdb mine qdrant --strategy-threshold 0`
- **THEN** Mine 运行产出 ≥ 5 个唯一缺陷（按 endpoint+param+DefectKind 三元组去重）
- **AND** 每个缺陷需通过至少 1 次 repro 验证（重新执行探针脚本确认 exit code == 1）

## MODIFIED Requirements

### Requirement: 合同提取管道
原两阶段提取流程扩展为三阶段：参数提取 → 约束提取 → 参数遗漏检测与补全。第三阶段使用 OpenAPI spec 作为权威源，通过参数列表比对识别遗漏，用 schema 信息直接补全约束。与现有 `augment_contract()` 互补而非重复：Phase 3 在提取时检测遗漏（增量），`augment_contract()` 在运行时合并约束（幂等）。

### Requirement: 探针生成管道
原硬编码的 `generate_probe()` 函数改为内部调用 `QdrantProbeTemplate` 的脚本级方法。`EndpointType` 枚举新增 `Config` 变体，`classify_endpoint_type()` 的 fallback 从 `Search` 改为 `Config`。`ProbeTemplate` trait 从表达式级改为脚本级（方法返回完整 Python 脚本）。新增 `assemble_script()` 辅助函数减少脚本组装重复。9 个硬编码调用点按 P0→P4 优先级增量替换。

## REMOVED Requirements
无
