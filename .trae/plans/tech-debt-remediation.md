# TestVDB 技术债修复计划

> Based on: deep-dive 技术债评估 (2026-05-31)
> Created: 2026-05-31
> Status: ACTIVE
> Prerequisite: Oracle 性能优化计划 (conditional-branch-llm-next.md) 已完成

---

## RALPLAN-DR Summary

### Principles (4)

1. **外科手术式修改** — 只改必须改的，不重构不相关的代码
2. **回归安全** — 每步修改后 cargo test 必须通过，不引入新问题
3. **优先级驱动** — P0 先于 P1 先于 P2，每级内按影响面排序
4. **增量交付** — 每个修复独立可验证，不依赖其他修复

### Decision Drivers (Top 3)

1. **运行时稳定性** — .unwrap() 可能导致整个 Mine 流程崩溃，是最紧急的风险
2. **可维护性** — 重复代码和硬编码增加修改成本和出错概率
3. **PRD 闭环** — AC1/AC3 未满足是项目交付的阻塞项

### Viable Options

**Option A: 全量修复（P0+P1+P2）**
- Pros: 一次性消除所有技术债
- Cons: 改动量大，回归风险高，耗时较长

**Option B: 分层递进（P0 → P1 → P2，每层独立验证）** ← 推荐
- Pros: 每层独立验证，回归风险可控，可随时停止
- Cons: 需要多次 cargo test

**Option C: 仅修 P0**
- Pros: 最小改动，最低风险
- Cons: P1 问题持续积累

### Invalidation Rationale

Option A 的全量修复在当前阶段不合适——P2 改动（如 time.sleep→轮询、Vec 预分配）收益有限但改动面广。Option C 忽略了 P1 中 DB URL 重复和 psycopg2 凭证硬编码等实际风险。Option B 是最佳平衡。

---

## Requirements Summary

消除 deep-dive 评估发现的技术债，按 P0→P1→P2 分层递进修复，确保：
- 生产代码不再因 .unwrap() 导致运行时 panic
- 重复代码模式提取为共享函数/常量
- PRD AC1/AC3 闭环

## Acceptance Criteria

| AC | 内容 | 验证方法 |
|----|------|---------|
| AC1 | 生产代码中 .unwrap() 数量从 17+ 降至 ≤3（仅保留 Regex::new 等编译期常量） | `grep -rn "\.unwrap()" src/ --exclude="*test*"` |
| AC2 | DB URL 构建 `format!("http://{}:{}", ...)` 重复从 11 处降至 1 处（共享函数） | `grep -rn 'format!("http://{}:{}"' src/` |
| AC3 | psycopg2 连接字符串从 12+ 处降至 1 处（共享模板） | `grep -rn "psycopg2.connect" src/` |
| AC4 | pip 安装模板从 3 处降至 1 处（共享函数） | `grep -rn "pypi.tuna.tsinghua" src/` |
| AC5 | 所有修改后 cargo test 通过 | `cargo test` |
| AC6 | Milvus Mine 运行验证 Oracle 优化通用性（PRD AC3） | 运行 Milvus Mine，确认 Oracle 阶段时间显著降低 |
| AC7 | Boundary 探针并行化可行性评估完成（PRD AC1） | 产出评估文档或代码 |

---

## Implementation Steps

### Phase 1: P0 — 消除运行时 panic 风险

#### Step 1.1: 修复 orchestrator.rs 中的 .unwrap()

**文件**: `src/agent/orchestrator.rs`
**问题**: 4 处 `take_sandbox().unwrap()` + 1 处 `current_task.unwrap()`

| 行号 | 当前代码 | 修复方案 |
|------|---------|---------|
| L726 | `let task = current_task.unwrap()` | `let task = current_task.ok_or_else(|| anyhow!("no active task in LLM loop"))?` |
| L988 | `executor.take_sandbox().unwrap()` | `executor.take_sandbox().ok_or_else(|| anyhow!("no sandbox available for verification"))?` |
| L1331 | 同上 | 同上 |
| L1490 | 同上 | 同上 |

**验证**: `cargo test` + `grep -rn "take_sandbox().unwrap()" src/`

#### Step 1.2: 修复 tools.rs 中的 .unwrap()

**文件**: `src/agent/tools.rs`
**问题**: 1 处 `db_host.as_ref().unwrap()` + 3 处 `as_array().unwrap()`

| 行号 | 当前代码 | 修复方案 |
|------|---------|---------|
| L27 | `sandbox.db_host.as_ref().unwrap()` | `sandbox.db_host.as_ref().ok_or_else(|| anyhow!("sandbox has no db_host"))?` |
| L499 | `params["required"].as_array().unwrap()` | `params["required"].as_array().ok_or_else(|| anyhow!("'required' is not an array"))?` |
| L511 | 同上 | 同上 |
| L525 | 同上 | 同上 |

**验证**: `cargo test` + `grep -rn "\.unwrap()" src/agent/tools.rs`

#### Step 1.3: 修复 executor.rs 和 sandbox_runner.rs 中的 .unwrap()

**文件**: `src/agent/executor.rs`, `src/agent/sandbox_runner.rs`

| 文件 | 行号 | 修复方案 |
|------|------|---------|
| executor.rs L191 | `self.active_sandbox.take().unwrap()` | `.ok_or_else(|| anyhow!("no active sandbox"))?` |
| sandbox_runner.rs L21 | `sandbox.db_host.as_ref().unwrap()` | `.ok_or_else(|| anyhow!("sandbox has no db_host"))?` |

**保留不修**: executor.rs L32 的 `Regex::new(r"...").unwrap()` — 正则字面量编译期可验证，运行时不会失败

**验证**: `cargo test`

#### Step 1.4: 修复 mutation.rs 中的 .unwrap()

**文件**: `src/agent/vdbfuzz/mutation.rs`

| 行号 | 当前代码 | 修复方案 |
|------|---------|---------|
| L423 | `serde_json::to_string(&base_body).unwrap()` | `serde_json::to_string(&base_body).map_err(|e| anyhow!("serialize failed: {}", e))?` |
| L507 | `body.as_object_mut().unwrap()` | `body.as_object_mut().ok_or_else(|| anyhow!("body is not a JSON object"))?` |

**验证**: `cargo test`

#### Step 1.5: 修复 review/qdrant.rs 中的 .unwrap()

**文件**: `src/review/qdrant.rs`

| 行号 | 当前代码 | 修复方案 |
|------|---------|---------|
| L17 | `sandbox.db_host.as_ref().unwrap()` | `.ok_or_else(|| anyhow!("sandbox has no db_host"))?` |

**验证**: `cargo test`

---

### Phase 2: P1 — 消除重复代码和硬编码

#### Step 2.1: 提取 DB URL 构建函数

**新建**: `src/util.rs`（或在现有工具模块中添加）

```rust
pub fn build_db_url(host: &str, port: u16) -> String {
    format!("http://{}:{}", host, port)
}
```

**替换 11 处**: `orchestrator.rs`(1), `tools.rs`(2), `sandbox_runner.rs`(1), `review/qdrant.rs`(1), `review/milvus.rs`(1), `review/weaviate.rs`(1), `infra.rs`(2), `batch_runner.rs`(2)

**验证**: `cargo test` + `grep -rn 'format!("http://{}:{}"' src/` 应为 0

#### Step 2.2: 提取 psycopg2 连接模板

**在 `src/target/pgvector.rs` 中添加**:

```rust
pub fn pg_connection_string(host: &str) -> String {
    format!("dbname=testvdb user=postgres password=postgres host={} port=5432", host)
}
```

**替换 12+ 处**: `semantic.rs`(12), `pgvector.rs`(15+), `review/pgvector.rs`(1), `batch_runner.rs`(1)

**验证**: `cargo test` + `grep -rn "psycopg2.connect" src/` 应仅剩模板函数

#### Step 2.3: 提取 pip 安装模板

**在 `src/sandbox/manager.rs` 中添加**:

```rust
pub fn pip_install_cmd(packages: &[&str]) -> Vec<String> {
    let mut cmd = vec![
        "pip".to_string(), "install".to_string(),
        "--timeout".to_string(), "120".to_string(),
        "--retries".to_string(), "3".to_string(),
        "-i".to_string(), "https://pypi.tuna.tsinghua.edu.cn/simple".to_string(),
    ];
    cmd.extend(packages.iter().map(|s| s.to_string()));
    cmd
}
```

**替换 3 处**: `manager.rs`(2), `infra.rs`(1)

**验证**: `cargo test` + `grep -rn "pypi.tuna.tsinghua" src/` 应仅剩模板函数

#### Step 2.4: 提取 LLM 重试逻辑

**在 `src/agent/llm.rs` 中重构**:

将 `send_chat_json_mode` 和 `send_chat_with_tools` 的重试循环提取为共享的 `async fn retry_llm_call<F, T>(f: F) -> Result<T>` 泛型函数。

**验证**: `cargo test`

#### Step 2.5: 提取硬编码常量

**在 `src/agent/llm.rs` 顶部添加**:

```rust
const LLM_MAX_RETRIES: u32 = 3;
const LLM_JSON_TEMPERATURE: f32 = 0.1;
const LLM_TOOL_TEMPERATURE: f32 = 0.7;
```

**在 `src/sandbox/manager.rs` 顶部添加**:

```rust
const DB_READY_TIMEOUT_SECS: u64 = 60;
const DB_PROBE_INTERVAL_MS: u64 = 500;
const SIDECAR_WAIT_SECS: u64 = 5;
```

**验证**: `cargo test`

---

### Phase 3: P1 — PRD 闭环

#### Step 3.1: Milvus Mine 端到端验证（PRD AC3）

**操作**: 运行 Milvus Mine，验证 Oracle 优化通用性
**验证**: Oracle 阶段时间显著降低（对比优化前）

#### Step 3.2: Boundary 探针并行化可行性评估（PRD AC1）✅ 已完成

**结论**: 部分可行 — Boundary 探针逻辑可并行，需调整 Runner 容器架构

**推荐方案**: 多 Runner 容器并行（方案 A）
- 为每组 case 创建独立 Runner 容器（`create_shared_runner` 已有此能力）
- 使用 `tokio::task::JoinSet` 并行执行
- 推荐并行度 3

**预期加速比**:
- Boundary 单项: 1.9x - 2.9x
- 整体 Mine: 约 8%（仅 Boundary），扩展到全部生成器可达 20-30%

**实施复杂度**: 中等，约 80-120 行代码改动
**最大风险**: 并发 DB 请求引发假阳性（概率低，可缓解）
**向后兼容**: 完全兼容，并行度默认为 1（串行）

---

### Phase 4: P2 — 性能优化（可选）

#### Step 4.1: 串行→并行执行（infra.rs）

**前提**: Phase 3 Step 3.2 评估结论为可行

将 `run_generic_batch` 中的串行 for 循环改为 `futures::join_all` 并行执行，并行度 3-4。

**验证**: cargo test + 对比单批执行时间

#### Step 4.2: time.sleep() → 轮询（Python 脚本模板）

**范围**: 嵌入式 Python 脚本中 100+ 处 `time.sleep()` 硬编码等待

**方案**: 将关键等待（如集合创建后的就绪检查）改为轮询模式：
```python
for _ in range(30):
    r = requests.get(f'{BASE}/collections/{c}')
    if r.status_code == 200: break
    time.sleep(0.5)
```

**验证**: cargo test + 对比执行时间

---

## Risks and Mitigations

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| .unwrap() → anyhow? 改动引入新编译错误 | 低 | 中 | 每步修改后立即 cargo test |
| DB URL 构建函数签名变更导致调用点遗漏 | 低 | 中 | grep 验证所有调用点 |
| Milvus Mine 运行时间过长 | 中 | 低 | 仅关注 Oracle 阶段时间，不要求总时间 <1h |
| Boundary 并行化导致 Docker 资源竞争 | 中 | 中 | 先评估再实施，限制并行度 |

## Verification Steps

1. Phase 1 完成后: `grep -rn "\.unwrap()" src/ --include="*.rs" | grep -v test | grep -v "#\[derive\]"` 应 ≤3
2. Phase 2 完成后: `cargo test` 通过 + grep 验证重复代码消除
3. Phase 3 完成后: Milvus Mine 结果文件 + 评估文档
4. 每步完成后: `cargo test` 必须通过

## ADR

### Decision
分层递进修复技术债（P0→P1→P2），每层独立验证

### Drivers
1. 运行时稳定性（.unwrap() panic 风险）
2. 可维护性（重复代码和硬编码）
3. PRD 闭环（AC1/AC3）

### Alternatives Considered
- 全量修复：改动量大，回归风险高
- 仅修 P0：P1 问题持续积累

### Why Chosen
分层递进平衡了修复速度和回归风险，每层可独立验证和交付

### Consequences
- Phase 1 完成后 Mine 流程不再因 .unwrap() 崩溃
- Phase 2 完成后代码可维护性显著提升
- Phase 3 完成后 PRD 可闭环

### Follow-ups
- Phase 4 的并行化和轮询优化可根据 Phase 3 评估结论决定是否实施
- semantic_gate.rs 的 JSON 解析 TODO 可在后续迭代中修复
