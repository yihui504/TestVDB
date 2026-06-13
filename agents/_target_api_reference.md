# 目标 API 参考（契约驱动 — 通用原则）

> 共享参考。攻击 Agent 必须**契约驱动**，禁止硬编码任何 DB 的端口/路径/语法/数据字段。
> ⛔ 不要写 per-DB 的 if/else 分支或写死表——那会把"硬编码 qdrant"换成"硬编码 4 个 DB"，版本变化时会过时并误导，且新增 DB 时会崩溃。

## 核心原则

1. **唯一真理源 = `structured_contract.json`**。从契约读取一切 DB 特定信息：
   - `target` 字段 → 当前 DB（weaviate / qdrant / milvus / pgvector）
   - `api_endpoints` → 端点路径（method + path + category + parameters + source_url）
   - `data_types` → 数据结构（字段命名、向量格式，如 weaviate 的 `properties`/`Class`/`vector`）
   - `constraints` / `assertions` → 待测约束与预期行为
2. **禁止硬编码 DB 特定值**：不写死端口（6333/8080）、不写死路径（`/collections/x/points`）、不写死数据字段（`payload`）、不写死过滤语法（`must`/`match`）、不写死响应键（`result`）。这些一律从契约推导或用占位符。
3. **示例代码用占位符**：路径写成 `<path from contract for X>`，并注释"从 `contract.api_endpoints` 读取；请求体/响应解析依据 `contract.target` 与 `contract.data_types` 推导"。
4. **BASE_URL 从环境变量**：`TESTVDB_DB_URL`（由 docker-executor 设置正确端口），未设置则 `VERDICT: SCRIPT_ERROR` 退出。**禁止任何默认端口**。
5. **响应解析通用化**：先 `print(raw_text)`，以 HTTP `status_code` 判定缺陷为主；响应体解析作为辅助，按 `contract.target` 动态选择键名，不要假设固定结构。
6. **target 来源 = 契约**：若脚本需要 target 变量，从 `structured_contract.json` 的 `target` 字段读取（**不要**用 `os.environ.get("TESTVDB_TARGET", ...)` 带默认值——默认值会假设错误 DB）。

## 为何不写 per-DB 语法表
不同 DB 版本的端点路径/请求体语法会变化；写死表会过时、会误导、新增 DB 时 `else: raise` 会让脚本崩溃。契约已包含 `target` + `api_endpoints` + `data_types`，足够 LLM 据此推导出当前 target 的正确语法。

## 参考样板
`agents/attack-boundary.md` 已采用此契约驱动模式（占位符 + 从契约读取，0 个 if/else TARGET 分支）。`attack-state.md` 与 `attack-semantic.md` 应遵循同一模式。
