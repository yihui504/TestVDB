# TestVDB 开发计划

> 生成时间：2026-05-23  
> 基于 Deep Interview 确认，所有方案已获批准。

---

## 一、当前状态

### 已具备

- Milvus 2.6.16 Shadow Mode 验证完成（Mine 96 缺陷 vs Batch 24 缺陷）
- 20 个 GitHub Issue 已提交到 milvus-io/milvus（5 个 P0 中 3 个已 CLOSED）
- 四库 TargetPlugin 架构就绪（Milvus/Qdrant/Weaviate/PGVector）
- 9 种确定性测试生成器 + LLM 编排器
- Docker 沙箱完整生命周期管理

### 核心瓶颈

| 链路 | 评分 | 根因 |
|------|------|------|
| 爬取→合同 | 3/10 | 只爬 TOC 第一页；无 JS 渲染；严重依赖预写资产 |
| LLM 合同提取 | 5/10 | 单次调用；无验证回路；Knowledge Agent 很少触发 |
| Harness 工程 | 6/10 | Docker CLI 依赖；无缓存；多库探针兼容差 |

> **关键发现**：Milvus 的 96 个缺陷产出主要依赖预写的 OpenAPI spec（5455 行）和行为模板（1225 行），而非实时爬取+LLM 提取。其他三库缺乏此类资产，整个测试链条从第一环断裂。

---

## 二、Phase A：Qdrant 全流程验证（目标：第一份非 Milvus 缺陷报告）

> **依赖**：计划一 + 计划二  
> **验收**：Qdrant 完成 Shadow Mode 对比，产出缺陷报告

### A1. 多页面递归爬取

**目标**：`run_extract` 从爬一页变为爬完整文档树。

**实现**：
- 修改 `contract_loader.rs` 中 `run_extract()` 的爬取逻辑
- `resolve_first_link()` → `crawl_all_links()`：BFS 递归爬取所有 TOC 链接 + 页面内链
  - 限制：同域、同路径前缀、深度 ≤ 3、上限 50 页
  - 黑名单过滤：GitHub、博客、社区论坛等非文档域名
- ChromiumCrawler 作为默认爬虫，`wait_for_selector("main,article,.content")` 等待渲染
- Chromium 不可用时自动降级到 ReqwestCrawler + warn 日志
- 爬取结果持久化为 `contracts/{target}_crawled_pages.json`

**验收**：
- [ ] `cargo run -- extract --target qdrant --docs-url https://qdrant.tech/documentation/ --out-dir contracts/` 爬取 ≥ 20 个页面
- [ ] 每个页面 Markdown > 500 字符
- [ ] 增量模式：重复运行时跳过已爬页面（URL + 时间戳比对）
- [ ] Chromium 降级时有明确 warn 日志

### A2. OpenAPI spec 自动发现

**目标**：爬取时自动检测并下载 OpenAPI spec。

**实现**：
- 在爬取完成后，对文档域名的以下路径发起 GET 请求：
  - `/openapi.json`
  - `/swagger.json`
  - `/api-docs`
  - `/api/openapi.json`
- 成功则保存为 `contracts/{target}_openapi.json`
- Qdrant 已知有 `https://api.qdrant.tech/openapi.json`

**验收**：
- [ ] `cargo run -- extract --target qdrant ...` 自动下载 Qdrant OpenAPI spec
- [ ] 保存路径 `contracts/qdrant_openapi.json`
- [ ] 若所有路径均 404，日志提示无 OpenAPI spec，不报错

### A3. 分阶段 LLM 合同提取

**目标**：单次 LLM 调用 → 两阶段调用（参数列表 → 约束提取），提高准确率。

**实现**：
- **阶段一**：LLM 输入完整 Markdown → 输出参数列表 JSON
  - Schema：`[{ "param_name": "search.limit", "endpoint": "search", "appears_in": "request body", "doc_description": "..." }]`
- **阶段二**：LLM 输入参数列表 + 原 Markdown → 输出完整 StructuredContract
  - 注入 5 个正例 + 3 个反例（从 milvus_contract.json 摘录）
- 若两阶段效果不优于单次（人工抽查 10 条准确率 < 80%），回退为单次增强 prompt

**验收**：
- [ ] 阶段一输出 ≥ 30 个参数
- [ ] 阶段二对每个参数输出 ≥ 1 个约束
- [ ] 人工抽查 10 个约束，准确率 ≥ 80%

### A4. 合同交叉验证

**目标**：自动检测合同内部矛盾。

**实现**：
- 程序化验证（不调用 LLM）：
  - 类型冲突：`type_constraint` 声明为 integer 但 OpenAPI spec 声明为 string → 标记
  - 缺失覆盖：`range_constraint` 缺少对应 `type_constraint` → 标记
  - 冗余约束：同一参数出现矛盾的类型约束 → 标记
- 输出 `contracts/{target}_validation_report.json`

**验收**：
- [ ] 对 Qdrant 合同跑验证，输出报告
- [ ] 无 OpenAPI spec 时跳过 OpenAPI 比对部分，不报错

### A5. Knowledge Agent 默认启用

**目标**：contracts 为空或合同断言 < 20 条时自动触发 Knowledge Agent。

**实现**：
- TargetPlugin trait 增加两个方法：
  - `fn default_repo_url(&self) -> Option<&str>`
  - `fn default_docs_url(&self) -> Option<&str>`
- 四个 plugin 实现返回已知默认值
- `load_contract_content()` 中：若 contracts_dir 为空 + 本地合同不存在或断言 < 20 → 自动调用 `run_knowledge_agent()`

**验收**：
- [ ] `cargo run -- mine --target qdrant --version v1.13.0`（不传 --contracts）自动走 Knowledge Agent
- [ ] 若默认 URL 也为空，输出清晰错误而非 panic

### A6. Qdrant Shadow Mode

**目标**：对 Qdrant 跑完整 Shadow Mode（Batch + Mine），产出第一份非 Milvus 对比报告。

**实现**：
- `cargo run -- mine --target qdrant --version v1.13.0 --shadow --skip-verify`
- 如 Qdrant Docker 网络已存在，Batch 模式直接复用

**验收**：
- [ ] Batch 模式探针 ≥ 20 个可用（通过 A7 实现）
- [ ] Mine 模式发现 ≥ 5 个唯一缺陷
- [ ] 生成 `results/qdrant/shadow_mode_report.md`

### A7. 探针模板抽象化

**目标**：通用探针逻辑复用，减少 per-DB 硬编码。

**实现**：
- 定义 `ProbeTemplate` trait（5 个核心操作）：

```rust
trait ProbeTemplate {
    fn base_url(&self) -> &str;
    fn auth_headers(&self) -> &str;
    fn create_collection_script(&self, name_var: &str, dim: u32) -> String;
    fn insert_data_script(&self, coll_var: &str, id: u64, vector: &str) -> String;
    fn search_script(&self, coll_var: &str, vector: &str, extra_params: &str) -> String;
    fn describe_script(&self, coll_var: &str) -> String;
    fn drop_script(&self, coll_var: &str) -> String;
}
```

- Milvus 和 Qdrant 分别实现
- 通用 `search_probe()` 函数通过 trait 动态分发
- 先抽象 5 个核心操作，验证通用性后再逐步迁移其余硬编码探针

**验收**：
- [ ] Milvus 现有探针通过 trait 实现，行为不变（回归测试）
- [ ] Qdrant 探针数 ≥ 20 个（不含硬编码，全部通过 trait + 通用函数生成）

---

## 三、Phase B：Weaviate 探针修复（目标：baseline 从 11 ERROR → 0 ERROR）

> **依赖**：Phase A 的爬取 + 探针抽象化  
> **验收**：Weaviate 基线探针全部通过

### B1. Weaviate 合同扩展

- 复用 A1/A2/A3 的爬取和提取管道
- Weaviate 文档爬取 + OpenAPI 自动发现
- 合同从 39 行扩展到 ≥ 200 行

**验收**：
- [ ] `cargo run -- extract --target weaviate --docs-url https://weaviate.io/developers/weaviate --out-dir contracts/`
- [ ] 生成的 `weaviate_contract.json` 含 ≥ 200 行有效约束

### B2. Weaviate 探针修复

- 基于 ProbeTemplate trait 为 Weaviate 实现 API 适配层
- 修复当前 11/11 ERROR 的根因（API 路径、认证方式、响应格式差异）
- 添加 smoke test：`batch` 命令启动时先发一个合法请求确认 API 可达

**验收**：
- [ ] `cargo run -- batch --target weaviate` 探针通过率 ≥ 80%
- [ ] smoke test 失败时给出明确诊断（如 "Weaviate not reachable at http://weaviate-standalone:8080"）

---

## 四、Phase C：PGVector 基础覆盖

> **依赖**：Phase A 的爬取 + 探针抽象化  
> **注意**：PGVector 无 OpenAPI spec，走纯 SQL 约束路径

### C1. PGVector 合同提取

- PGVector 文档爬取（GitHub README + SQL 参考文档）
- 约束从 SQL 语义提取（DDL 约束、类型系统、索引参数范围）
- 不需要 OpenAPI spec

**验收**：
- [ ] `pgvector_contract.json` 从 36 行扩展到 ≥ 150 行

### C2. PGVector 探针基础验证

- 基于 ProbeTemplate trait（SQL 适配模式）实现 PGVector 插件
- 至少 10 个可用探针

**验收**：
- [ ] `cargo run -- batch --target pgvector` 至少 10 个探针运行，通过率 ≥ 50%

---

## 五、Phase D：横向对比与论文数据

> **依赖**：Phase A+B+C 全部完成  
> **验收**：四库对比报告 + 论文核心实验数据

### D1. 四库横向对比

- 统一运行：`cargo run -- mine --target {milvus,qdrant,weaviate,pgvector} --shadow --skip-verify`
- 对比指标：唯一缺陷数、缺陷类型分布、探针通过率、Mine vs Batch 倍率
- 输出 `results/comparison_report.md`

### D2. 论文数据

- 从结构化结果存储中提取核心实验数据
- 撰写"方法 → 实验设计 → 结果分析"三个章节的数据骨架

**验收**：
- [ ] `results/comparison_report.md` 生成完成
- [ ] 论文数据骨架输出为 `docs/paper_data.md`

---

## 六、Harness 加固（贯穿全阶段）

> 以下改进在所有 Phase 中持续进行。

### H1. Docker 镜像预热缓存

- `--cache-images` 标志
- 首次 `docker pull` + `pip install` 后标记已缓存
- 二次运行检测镜像存在 → 跳过 pull；检测 pip 包已安装 → 跳过 install

**验收**：
- [ ] 二次运行迭代周期 < 1 分钟（不含首次）
- [ ] `--cache-images` 未传时行为不变（每次拉取）

### H2. 结构化结果存储

- 输出目录：`results/{target}/{version}/{timestamp}/`
- 文件：`contract.json`, `defects.jsonl`, `coverage.json`, `run.log`
- 自动生成 `summary.md`

**验收**：
- [ ] 运行后 `results/` 目录结构符合规范
- [ ] 根目录旧日志归档到 `results/archive/`

### H3. 低产出策略自动暂停

- 当 ContractStore 约束数 < 阈值（默认 100）时自动跳过 state/meta/seq/res/combo/conc 策略
- 可通过 `--strategy-threshold 0` 恢复全部

**验收**：
- [ ] 约束 < 阈值时日志显示跳过原因
- [ ] `--strategy-threshold 0` 恢复全部策略

### H4. 跨库探针自检（Smoke Test）

- `batch` 命令启动时对目标 DB 发送一个合法请求
- 失败时给出明确诊断而非静默失败

**验收**：
- [ ] Milvus smoke test 通过（`POST /v2/vectordb/collections/list`）
- [ ] Qdrant smoke test 通过（`GET /collections`）
- [ ] Weaviate smoke test 失败时输出 "Weaviate not reachable at ..."

---

## 七、时间线估算

| Phase | 内容 | 预估 |
|-------|------|------|
| A1-A5 | 爬取+提取管道改造 | 1 周 |
| A6-A7 | Qdrant Shadow Mode + 探针抽象 | 1 周 |
| B1-B2 | Weaviate 合同+探针修复 | 1 周 |
| C1-C2 | PGVector 基础覆盖 | 1 周 |
| D1-D2 | 横向对比+论文数据 | 1 周 |
| H1-H4 | Harness 加固 | 穿插进行 |

**总计**：约 5-6 周（Harness 加固与 Phase 并行，不额外增加时间）。

---

## 八、风险与假设

| 风险 | 缓解 |
|------|------|
| Qdrant/Weaviate 文档是 SPA，Chromium 模式资源消耗大 | 优先尝试 Reqwest 静态抓取，不命中则降级 Chromium |
| PGVector 无 OpenAPI，合同质量受限 | 走 SQL DDL 解析路径，辅以 LLM 从 README 提取 |
| 两阶段 LLM 提取成本高于单阶段 | 设准确率阈值 80%，不达标回退 |
| Qdrant/Weaviate 维护者不接受缺陷报告格式 | 沿用 Milvus 已验证的 Issue 模板 |
| Chromium 在 Docker 沙箱中不可用 | ReqwestCrawler 降级 + 日志 warn，不阻塞流程 |
