# Tasks

- [x] Task 1: A7 — ProbeTemplate 脚本级重构与硬编码调用点替换（最高优先级）

  - [x] SubTask 1.1: 实现 `assemble_script(preamble, setup_lines, test_step) -> String` 辅助函数
  - [x] SubTask 1.2: 重构 ProbeTemplate trait 为脚本级：方法签名改为 `fn search_probe(&self, param: &str, value: &str, label: &str) -> String` 等，返回完整 Python 脚本
  - [x] SubTask 1.3: QdrantProbeTemplate 实现脚本级方法（search_probe, create_probe, upsert_probe, delete_probe, scroll_probe, recommend_probe, search_params_probe, update_config_probe），内部使用 `assemble_script()`
  - [x] SubTask 1.4: MilvusProbeTemplate 实现脚本级方法（search_probe, create_probe, insert_probe, query_probe, search_params_probe），内部使用 `assemble_script()`
  - [x] SubTask 1.5: 在 `EndpointType` 枚举中新增 `Config` 变体，修改 `classify_endpoint_type()` 的 fallback 从 `Search` 改为 `Config`
  - [x] SubTask 1.6: 在 `TargetPlugin` trait 中添加 `probe_template(&self) -> &dyn ProbeTemplate` 方法
  - [x] SubTask 1.7: 更新 `QdrantPlugin` 和 `MilvusPlugin` 实现 `probe_template()`

  - [x] SubTask 1.8: **P0.1 — 增量替换 generate_probe()**：重构 `generate_probe()` 内部调用 `QdrantProbeTemplate` 方法 → golden output 对比 + `cargo test` 验证 → 打 git tag `a7-p0-done`
  - [x] SubTask 1.9: **P0.2 — SimpleSafetyNet::to_safety_net()**：确认已通过 P0.1 自动适配（它调用 `generate_probe()`）→ `cargo test` 验证

  - [x] SubTask 1.10: **P1 — 根因修复 OracleCheckDeriver**：`OracleCheckDeriver::from_range_constraints()` 改为调用 `generate_probe()` 或直接使用 ProbeTemplate → golden output 对比 + `cargo test` 验证 → 打 git tag `a7-p1-done`

  - [x] SubTask 1.11: **P2.1 — make_case() 替换**：`boundary.rs::make_case()` 通过 ProbeTemplate 生成脚本 → `cargo test` 验证
  - [x] SubTask 1.12: **P2.2 — milvus_generate_probe() 替换**：统一到 `MilvusProbeTemplate` → `cargo test` 验证
  - [x] SubTask 1.13: **P2.3 — qdrant_float_probe 等替换**：`qdrant_float_probe()` / `qdrant_missing_required_probe()` / `qdrant_invalid_enum_probe()` 通过 ProbeTemplate 生成 → `cargo test` 验证 → 打 git tag `a7-p2-done`

  - [x] SubTask 1.14: **P3 — MilvusSimpleSafetyNet 增量迁移**：逐方法内部改用 `MilvusProbeTemplate`，保留方法结构不变 → `cargo test` 验证 → 打 git tag `a7-p3-done`

  - [x] SubTask 1.15: **P4 — 长尾清理**：`diff_concurrent.rs` 和 `sequence_gen.rs` 中的硬编码探针脚本通过 ProbeTemplate 生成 → `cargo test` 验证 → 打 git tag `a7-p4-done`

  - [x] SubTask 1.16: 新增 golden output 对比测试：`QdrantProbeTemplate::search_probe()` 输出与 `search_probe()` 完全相同
  - [x] SubTask 1.17: 新增 golden output 对比测试：`MilvusProbeTemplate::search_probe()` 输出与 `milvus_search_probe()` 完全相同
  - [x] SubTask 1.18: 新增测试：`QdrantProbeTemplate::update_config_probe()` 包含 PATCH 方法
  - [x] SubTask 1.19: 新增测试：`classify_endpoint_type("quantization_config", "...")` 返回 `EndpointType::Config`
  - [x] SubTask 1.20: 最终 `cargo build` + `cargo test` 全量验证

- [x] Task 2: A2 — OpenAPI 自动发现增强
  - [x] SubTask 2.1: 在 `run_extract()` 的 probe_origins 中补充 `api.qdrant.tech` 子域
  - [x] SubTask 2.2: 在 probe_paths 中补充 `/redoc/openapi.json`、`/api/v1/openapi.json` 等路径
  - [x] SubTask 2.3: 验证自动发现成功保存 `qdrant_openapi.json`
  - [x] SubTask 2.4: 验证 `OpenApiParser::extract_to_contract_store()` 产出的 type+range constraints ≥ 20 条

- [x] Task 3: A3 — 实现第三阶段参数遗漏检测（与 augment_contract 互补）
  - [x] SubTask 3.1: 在 `contract_loader.rs` 中实现 `run_phase3_param_gap_detection()` 函数：从 OpenAPI spec 提取完整参数列表，比对 LLM 产出参数，识别遗漏
  - [x] SubTask 3.2: 对遗漏参数，用 OpenAPI spec 的 schema 信息直接补全 type_constraints 和 range_constraints
  - [x] SubTask 3.3: 实现降级策略：无 OpenAPI spec 时，对 Phase 2 assertion 做结构化解析验证，解析失败的单条回退 LLM 重格式化
  - [x] SubTask 3.4: 在 `run_extract()` 中调用 Phase 3（在 Phase 2 之后、`augment_contract()` 之前），将补全结果合并到合同
  - [x] SubTask 3.5: 输出补全报告（新增多少参数/约束）
  - [x] SubTask 3.6: 验证 Qdrant 合同 assertions ≥ 30、type+range constraints ≥ 15、参数级召回率 ≥ 80%

- [x] Task 4: A1 — BFS 爬取验证与调优（降级为补充手段）
  - [x] SubTask 4.1: 实际运行 `testvdb extract qdrant https://qdrant.tech/documentation/ contracts`，观察爬取页数（跳过 - A2+A3 已达到 90% 召回率）
  - [x] SubTask 4.2: 若 <20 页，调优 `crawl_docs_site` 参数（max_pages、max_depth、path_prefix 逻辑）（跳过）
  - [x] SubTask 4.3: 验证增量模式合并逻辑正确（跳过）

- [x] Task 5: A6 — Qdrant Shadow Mode（分步执行）
  - [x] SubTask 5.1: A6.1 — 执行 `testvdb batch qdrant`，验证 ≥20 探针通过率 ≥60%
  - [x] SubTask 5.2: A6.2 — 量化 Go/No-Go：通过率 ≥60% 且假阳性率 <30%（假阳性 = stdout 含 [DEFECT: 但 exit code != 1）
  - [x] SubTask 5.3: A6.3 — 执行 `testvdb mine qdrant --strategy-threshold 0`，验证产出 ≥5 唯一缺陷（每个需 repro 验证）
  - [x] SubTask 5.4: 整理缺陷报告，更新 `results/qdrant/` 目录

# Task Dependencies
- Task 1 (A7 探针管道) 无前置依赖，最先执行
- Task 2 (A2 OpenAPI) 无前置依赖，可与 Task 1 并行
- Task 3 (A3 参数遗漏检测) 依赖 Task 2（需要 OpenAPI spec 作为权威源）
- Task 4 (A1 爬取) 依赖 Task 3（仅在 A2+A3 不够时启用）
- Task 5 (A6 Shadow Mode) 依赖 Task 1 + Task 3（管道修复 + 合同扩展后才能重跑）
- Task 1 和 Task 2 可并行执行
- Task 1 内部：P0 → P1 → P2 → P3 → P4 严格顺序执行

# ADR — ProbeTemplate 脚本级设计决策

## Decision
ProbeTemplate trait 方法返回完整 Python 脚本（而非表达式片段），配合 `assemble_script()` 辅助函数减少重复。

## Drivers
1. 现有探针函数（search_probe, create_probe 等）均返回完整脚本，脚本级 trait 可 1:1 替换
2. 表达式级 trait 需要额外组装器，增加间接层但未减少复杂度
3. Milvus 的 15+ 操作类型各有独特 setup 逻辑，通用组装器难以覆盖

## Alternatives Considered
1. **表达式级 + 组装器（Option B）**：保留当前表达式级 trait，新增 ProbeScriptAssembler。被否决因为组装器本质上重新实现了现有探针函数的逻辑。
2. **Setup DSL + ProbeOperation 枚举（Option C）**：引入 ScriptStep/EndpointRequest/ProbeOperation 5 个新类型 + assemble_probe() 函数。被否决因为过度工程化——5 个新类型对于一个 0 缺陷项目太重。

## Why Chosen
脚本级 trait + `assemble_script()` 辅助函数是最小化改动方案：
- 类型系统零新增（仅改 trait 方法签名 + 1 个辅助函数）
- 增量替换路径清晰（P0→P4 每步可验证）
- Golden output 对比确保行为等价

## Consequences
- ProbeTemplate 方法体较长（包含完整脚本模板），但可通过 `assemble_script()` 提取公共部分
- 新增 EndpointType 变体时需同时更新 ProbeTemplate 方法，但这是显式依赖而非隐式耦合

## Follow-ups
- Milvus 的 15+ 操作类型（partition, alias, database 等）暂不在 ProbeTemplate 中覆盖，保留 `milvus_*_probe()` 函数，P3 阶段逐方法迁移
- 未来可考虑引入 `ProbeOperation` 枚举替代 `EndpointType`，但不在 A7 范围内
