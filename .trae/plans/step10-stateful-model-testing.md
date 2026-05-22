# Step 10: 有状态模型测试 — execute_stateful_test 工具

**Created:** 2026-05-21
**Updated:** 2026-05-22
**Status:** PENDING (Deep Interview 完成，模糊度 13.3%)
**Predecessor:** Step 9 (COMPLETED, V30)
**Parent Plan:** `.trae/plans/diversity-enhancement-plan.md`
**Spec:** `.trae/specs/deep-interview-llm-orchestrator-v2.md` (Phase 2)

---

## 1. 目标

让 LLM 编排器从"参数边界测试"转向"多步状态交互测试"，产出确定性生成器无法发现的增量 Bug。

**核心洞察**：当前 `execute_api_sequence` 只生成线性串行脚本（step1→step2→...→invariant_check），每步只检查 `code != 0`，无法表达"insert 100 条 → delete 50 条 → rowCount 是否等于 50"这类状态一致性验证。新工具 `execute_stateful_test` 让 LLM 声明"模型-实现"对比测试，工具自动在每步操作后验证实际状态与预期一致。

---

## 2. 根因诊断

### 2.1 当前瓶颈（3层）

| 层级 | 问题 | 影响 |
|------|------|------|
| **工具设计** | `execute_api_sequence` 生成线性脚本，无状态验证 | LLM 无法表达状态一致性检查 |
| **不变量检查** | 只检查 `count < 0` 和 `dimension <= 0` | 无法检测语义级缺陷（rowCount 不一致、数据残留、排序错误） |
| **Prompt 导向** | 10 种模式类别本质是参数边界变体 | LLM 探索方向与确定性生成器重叠 |

### 2.2 Milvus 已知的状态交互 Bug（确定性生成器无法发现）

| Bug 模式 | 具体表现 | Issue |
|---------|---------|-------|
| Flush-Query 竞态 | flush 后数据 3 分钟不可见 | #47913 |
| Load-Search 竞态 | load 返回成功但 search 失败 | #47635 |
| 并发读写 Panic | nil 指针解引用 | #42723 |
| 异步加载死锁 | 集合加载永久阻塞 | #41993 |

---

## 3. 技术方案

### 3.1 新工具：`execute_stateful_test`

**灵感来源**：Hypothesis RuleBasedStateMachine + quickcheck-state-machine

**核心思想**：LLM 声明操作序列 + 每步的预期状态变化，工具自动生成 Python 脚本，在每步操作后验证实际状态与模型预期一致。

#### 3.1.1 工具定义（tools.rs）

```rust
pub fn get_execute_stateful_test_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_stateful_test".to_string(),
            description: Some(
                "STATEFUL MODEL TESTING. Tests multi-step API sequences with automatic state verification. \
                 Unlike execute_api_sequence (which only checks response codes), this tool verifies that \
                 the actual database state matches the expected model state after EACH operation. \
                 Example: insert 100 entities → verify rowCount=100 → delete 50 → verify rowCount=50. \
                 Use this to find STATE_LOGIC_VIOLATION bugs that deterministic generators cannot detect.".to_string()
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_name": {
                        "type": "string",
                        "description": "Descriptive name (e.g., 'insert_delete_rowcount_consistency')"
                    },
                    "pattern_category": {
                        "type": "string",
                        "enum": [
                            "count_consistency",       // insert N → verify count, delete M → verify count
                            "data_visibility",         // insert → flush → search, verify results visible
                            "state_residual",          // drop → recreate → verify old state gone
                            "idempotency",             // same operation twice → verify no side effect
                            "search_correctness",      // insert known data → search → verify ordering/limits
                            "partition_isolation",     // insert partition A → search partition B → verify no leak
                            "alias_state",             // create alias → drop collection → verify alias broken
                            "index_state"              // create index → drop → recreate different → verify
                        ],
                        "description": "The state interaction pattern being tested"
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "description": "API endpoint (e.g., '/v2/vectordb/entities/insert')"
                                },
                                "params": {
                                    "type": "object",
                                    "description": "Request parameters"
                                },
                                "expect_success": {
                                    "type": "boolean",
                                    "description": "Whether this step should succeed (true) or fail (false)"
                                },
                                "state_check": {
                                    "type": "object",
                                    "properties": {
                                        "method": {
                                            "type": "string",
                                            "enum": [
                                                "describe_collection",  // check rowCount, dimension, etc.
                                                "query_entities",       // check specific entity data
                                                "search_results",       // check search result count/ordering
                                                "list_collections",     // check collection exists/not exists
                                                "get_index"             // check index state
                                            ],
                                            "description": "How to verify the state after this step"
                                        },
                                        "expected": {
                                            "type": "object",
                                            "description": "Expected state values. Examples: {\"rowCount\": 100}, {\"exists\": false}, {\"resultCount\": 5}, {\"distancesAscending\": true}"
                                        }
                                    },
                                    "required": ["method", "expected"],
                                    "description": "State verification to perform after this step. Omit to skip verification for this step."
                                }
                            },
                            "required": ["action", "params", "expect_success"]
                        },
                        "description": "Ordered list of steps with state checks"
                    },
                    "invariant": {
                        "type": "string",
                        "description": "Final invariant to verify after all steps (e.g., 'rowCount must equal insertCount - deleteCount')"
                    }
                },
                "required": ["test_name", "pattern_category", "steps"]
            }),
        },
    }
}
```

#### 3.1.2 脚本生成逻辑（orchestrator.rs）

当 LLM 调用 `execute_stateful_test` 时，编排器生成如下结构的 Python 脚本：

```python
# Stateful Model Test: {test_name}
import requests, sys, uuid, time, json
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
COLL = 'stf_' + uuid.uuid4().hex[:8]

def api(path, body):
    try:
        r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body, timeout=30)
        return r.json()
    except Exception as e:
        return {'code': -1, 'message': f'Request failed: {e}'}

def get_state(method, collection_name=None):
    """Query current database state based on method."""
    if method == 'describe_collection':
        r = api('/v2/vectordb/collections/describe', {'collectionName': collection_name})
        return r.get('data', {})
    elif method == 'list_collections':
        r = api('/v2/vectordb/collections/list', {})
        return r.get('data', [])
    elif method == 'query_entities':
        # Will be customized per step
        pass
    elif method == 'search_results':
        # Will be customized per step
        pass
    elif method == 'get_index':
        r = api('/v2/vectordb/indexes/describe', {'collectionName': collection_name})
        return r.get('data', {})
    return {}

defect_found = False

def verify_state(step_num, state_check, collection_name=None):
    """Verify actual state matches expected model state."""
    global defect_found
    method = state_check.get('method')
    expected = state_check.get('expected', {})
    actual = get_state(method, collection_name)

    for key, expected_val in expected.items():
        actual_val = actual.get(key)
        if key == 'rowCount':
            if actual_val != expected_val:
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: rowCount={actual_val}, expected {expected_val}')
                defect_found = True
        elif key == 'exists':
            collection_exists = any(
                c.get('name') == collection_name
                for c in (actual if isinstance(actual, list) else [])
            )
            if collection_exists != expected_val:
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: collection exists={collection_exists}, expected {expected_val}')
                defect_found = True
        elif key == 'resultCount':
            # For search/query results
            count = len(actual) if isinstance(actual, list) else actual.get('count', -1)
            if count != expected_val:
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: resultCount={count}, expected {expected_val}')
                defect_found = True
        elif key == 'distancesAscending':
            # For L2 distance: should be ascending
            distances = [d.get('distance', 0) for d in (actual if isinstance(actual, list) else [])]
            if expected_val and distances != sorted(distances):
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: distances not ascending: {distances}')
                defect_found = True
        elif key == 'distancesDescending':
            # For IP/COSINE: should be descending
            distances = [d.get('distance', 0) for d in (actual if isinstance(actual, list) else [])]
            if expected_val and distances != sorted(distances, reverse=True):
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: distances not descending: {distances}')
                defect_found = True
        else:
            if actual_val != expected_val:
                print(f'[DEFECT: STATE_LOGIC_VIOLATION] Step {step_num}: {key}={actual_val}, expected {expected_val}')
                defect_found = True

# --- Step Execution ---

# Step 1: {action} (expect_success: {expect})
r1 = api('{endpoint}', {params_with_collection_name})
print('Step 1 result:', json.dumps(r1))
if {expect_success} and r1.get('code', -1) != 0:
    print(f'[DEFECT: SEQUENCE_VIOLATION] Step 1 expected success but got code={r1.get("code")}: {r1}')
    defect_found = True
if not {expect_success} and r1.get('code', -1) == 0:
    print(f'[DEFECT: ILLEGAL_SUCCESS] Step 1 expected error but succeeded: {r1}')
    defect_found = True
time.sleep(0.5)

# Step 1 state check: {method}
verify_state(1, {state_check_json}, COLL)

# Step 2: ...
# ...

# --- Final Invariant ---
{invariant_check_code}

if defect_found:
    sys.stdout.flush(); sys.exit(1)
print('All state checks passed')
sys.exit(0)
```

**关键差异**（与 `execute_api_sequence` 对比）：

| 维度 | execute_api_sequence | execute_stateful_test |
|------|---------------------|----------------------|
| 每步检查 | 只检查 `code != 0` | 检查 `code` + **实际状态 vs 预期状态** |
| 状态查询 | 无 | `describe_collection`/`query_entities`/`search_results` 等 |
| 不变量 | 只有 `count<0`/`dimension<=0` | rowCount、exists、resultCount、distancesAscending 等 |
| 缺陷类型 | SEQUENCE_VIOLATION/ILLEGAL_SUCCESS | **STATE_LOGIC_VIOLATION**（新增语义级检测） |

#### 3.1.3 特殊处理：insert/delete 的参数展开

LLM 声明 `"insert count=100"` 时，工具需要展开为 100 条 insert 数据。这通过以下方式实现：

- 如果 `params` 中包含 `"count": N`，工具自动生成 N 条向量数据
- 向量维度从 `describe_collection` 获取，或使用默认值 4
- 每条数据有唯一 ID（`uuid.uuid4().hex[:8]`）和随机向量

```python
# 自动展开 insert 的 count 参数
def generate_vectors(count, dimension=4):
    import random
    return [
        {"id": i+1, "vector": [round(random.uniform(0, 1), 4) for _ in range(dimension)]}
        for i in range(count)
    ]
```

---

## 4. 实施步骤

### 4.1 Step 10.1: 工具定义（tools.rs）

**文件**：`src/agent/tools.rs`
**改动**：新增 `get_execute_stateful_test_tool()` 函数

**验证**：`cargo build` 编译通过

### 4.2 Step 10.2: 脚本生成逻辑（orchestrator.rs）

**文件**：`src/agent/orchestrator.rs`
**改动**：
1. 在工具注册列表中添加 `execute_stateful_test`
2. 在 `match function_name` 分支中添加 `"execute_stateful_test"` 处理
3. 实现脚本生成逻辑：
   - 解析 `test_name`、`pattern_category`、`steps`、`invariant`
   - 生成带状态验证的 Python 脚本
   - 处理 `state_check` 中的各种 `method`
   - 处理 insert 的 `count` 参数展开

**验证**：`cargo build` 编译通过

### 4.3 Step 10.3: Prompt 更新（orchestrator.rs）

**文件**：`src/agent/orchestrator.rs`
**改动**：
1. System prompt 中添加 `execute_stateful_test` 工具描述
2. 替换模式类别为状态交互模式：

| 旧模式 | 新模式 | 目标 Bug |
|--------|--------|---------|
| collection_lifecycle | **count_consistency** | insert→delete→rowCount 不一致 |
| insert_search | **data_visibility** | flush→search 数据不可见 |
| drop_recreate | **state_residual** | drop→recreate→旧状态残留 |
| alias_lifecycle | **alias_state** | alias→drop→alias 仍可用 |
| partition_lifecycle | **partition_isolation** | 分区间数据泄漏 |
| index_lifecycle | **index_state** | 索引状态不一致 |
| upsert_query | **idempotency** | 重复操作产生副作用 |
| flush_persist | **data_visibility** | flush 后数据不可见 |
| delete_filter_compare | **count_consistency** | delete 后计数不一致 |
| param_equivalence | **search_correctness** | 搜索排序/数量错误 |

3. 更新探索策略表：

```
- Turn 1-2: Use execute_stateful_test with count_consistency pattern
- Turn 3-4: Use execute_stateful_test with data_visibility pattern
- Turn 5-6: Use execute_stateful_test with state_residual or idempotency pattern
- Turn 7-8: Use execute_stateful_test with search_correctness or partition_isolation
- Turn 9-10: Use execute_stateful_test with alias_state or index_state
- Turn 11: Use get_coverage_report() to find untested patterns
- Turn 12: Submit MRE if defects found
```

4. 更新 `MANDATORY RULES`：
   - Turn 1-3 必须使用 `execute_stateful_test`（不再允许 `execute_api_sequence`）
   - 每轮必须选择不同的 `pattern_category`

**验证**：`cargo build` 编译通过

### 4.4 Step 10.4: 模式追踪更新（coverage.rs）

**文件**：`src/agent/coverage.rs`
**改动**：
1. `PatternTracker` 的模式类别从旧的 10 种替换为新的 8 种
2. `infer_pattern_from_endpoints` 函数更新：从 `pattern_category` 参数直接获取模式（不再从端点推断）

**验证**：`cargo build` 编译通过

### 4.5 Step 10.5: 实战验证

**命令**：
```powershell
$env:DEEPSEEK_API_KEY="sk-..."
cargo run -- mine --target milvus --version v2.6.16 --contracts contracts --max-rounds 1 --llm-turns 8 --skip-generators
```

**验证清单**：
1. LLM 使用 `execute_stateful_test` 工具（而非 `execute_api_sequence`）
2. 生成的脚本包含 `verify_state()` 调用（状态验证）
3. 至少 1 个 `STATE_LOGIC_VIOLATION` 缺陷被发现
4. 缺陷通过完整验证流程（repro_1 + repro_2）
5. 缺陷是确定性生成器无法发现的增量 Bug

### 4.6 Step 10.6: V31 实战验证结果

**成功**：
- LLM 使用 `execute_stateful_test`（Turn 2, state_residual 模式）
- 模式追踪正确（state_residual + search_correctness）
- 完整验证流程通过（repro_1 + repro_2 + LLM 验证变体 + LLM 报告优化）

**失败**：
- LLM 未在 steps 中传入 `state_check`，工具退化为普通序列测试
- 最终缺陷仍是 Oracle 的参数边界问题（nprobe=0），非 STATE_LOGIC_VIOLATION

### 4.7 Step 10.7: state_check 歧义修复

**根因**：`state_check` 定义太弱，3 层问题：

| 优先级 | 问题 | 修复 |
|--------|------|------|
| P0 | `state_check` 是 optional 字段，LLM 跳过了它 | tools.rs: 改为 required（steps 中至少 1 个必须有 state_check） |
| P0 | Prompt 没有完整 JSON 调用示例 | orchestrator.rs: 添加 execute_stateful_test 的完整示例 |
| P1 | method→expected key 对应关系未说明 | orchestrator.rs Prompt: 添加对应表 |
| P1 | "when possible" 太弱 | orchestrator.rs Prompt: 改为 "in at least 1 step" |
| P2 | search_results 默认维度可能不匹配 | orchestrator.rs 脚本: 从 describe_collection 获取维度 |

**具体修复**：

#### P0-1: tools.rs — state_check 改为 required

将 steps items 的 required 从 `["action", "params", "expect_success"]` 改为 `["action", "params", "expect_success", "state_check"]`。

同时更新 description: "State verification to perform after this step. REQUIRED — this is what makes stateful testing unique."

#### P0-2: orchestrator.rs Prompt — 添加完整调用示例

```
=== EXAMPLE execute_stateful_test CALL ===
{
  "test_name": "insert_delete_rowcount",
  "pattern_category": "count_consistency",
  "steps": [
    {"action": "/v2/vectordb/collections/create", "params": {"collectionName": "COLL", "dimension": 4}, "expect_success": true, "state_check": {"method": "describe_collection", "expected": {"rowCount": 0}}},
    {"action": "/v2/vectordb/entities/insert", "params": {"collectionName": "COLL", "count": 10, "dimension": 4}, "expect_success": true, "state_check": {"method": "describe_collection", "expected": {"rowCount": 10}}},
    {"action": "/v2/vectordb/entities/delete", "params": {"collectionName": "COLL", "filter": "id in [1,2,3]"}, "expect_success": true, "state_check": {"method": "describe_collection", "expected": {"rowCount": 7}}}
  ],
  "invariant": "rowCount must equal insertCount - deleteCount"
}
```

#### P1-1: orchestrator.rs Prompt — method→expected 对应表

```
method                  | Available expected keys
describe_collection     | rowCount (int), dimension (int), indexCount (int)
list_collections        | exists (bool)
query_entities          | resultCount (int)
search_results          | resultCount (int), distancesAscending (bool), distancesDescending (bool)
get_index               | indexType (str), metricType (str)
```

#### P1-2: "when possible" → "in at least 1 step"

---

| 条件 | 验证方式 |
|------|---------|
| `execute_stateful_test` 工具定义完成 | `cargo build` 通过 |
| 脚本生成逻辑正确 | 生成的 Python 脚本包含 `verify_state()` |
| Prompt 更新完成 | LLM 使用新工具而非旧工具 |
| 模式追踪更新 | 8 种新模式类别被追踪 |
| LLM 使用新工具 | 实战日志中 `execute_stateful_test` 被调用 |
| 产出 STATE_LOGIC_VIOLATION 缺陷 | 实战运行中发现状态不一致 |
| 缺陷是增量的 | 确定性生成器无法发现同类缺陷 |
| `execute_api_sequence` 已移除 | 旧工具不再出现在工具列表中 |
| 复用现有验证流程 | 缺陷走 repro_1 → repro_2 → LLM 验证变体 → LLM 报告优化 |

### Phase 2 总体验收标准（来自 Deep Interview）

- **主标准**：≥3 个不同增量 Bug 类型 + 全部通过完整验证流程 + 至少 1 个提交为 GitHub Issue
- **降级标准**：3 次实战运行后仍无 3 个增量 Bug → 降级为"工具能力就绪 + ≥1 个增量 Bug 类型"

### 增量 Bug 类型枚举（来自 Deep Interview）

| # | Bug 类型 | 具体表现 | 对应工具 |
|---|---------|---------|---------|
| 1 | **计数不一致** | insert N→delete M→rowCount≠N-M；bulk_insert 重复主键→rowCount 虚高 | stateful + concurrent |
| 2 | **数据可见性异常** | flush→search 数据不可见；delete→search 仍返回已删除数据 | stateful + timing |
| 3 | **搜索结果错误** | L2 距离非升序；limit=5 返回 6 条 | stateful |
| 4 | **状态残留** | drop collection→recreate→旧数据残留 | stateful |
| 5 | **并发竞态** | 并发 insert→rowCount≠N；并发 upsert 同一 ID→出现重复 | concurrent |

---

## 6. 不在范围内

- `execute_concurrent_test`（Step 11）
- `execute_timing_test`（Step 12）
- 并发/竞态条件测试
- LLM 模型切换
- 确定性生成器改进

---

## 7. 风险与缓解

| 风险 | 缓解 |
|------|------|
| LLM 不使用新工具 | Prompt 强制 Turn 1-3 必须使用 `execute_stateful_test`；代码层拒绝 `execute_api_sequence` 在前 3 轮 |
| 生成的脚本 Traceback | `api()` 加 try/except；`verify_state()` 加异常处理 |
| 状态查询 API 不一致 | `get_state()` 函数统一处理不同 method 的响应格式 |
| insert count 展开太慢 | 限制 count ≤ 1000；批量 insert 而非逐条 |
| Milvus 无状态一致性 Bug | 接受可能性；即使无增量 Bug，工具能力提升仍有价值 |

---

## 8. 文件改动预估

| 文件 | 改动类型 | 预估行数 |
|------|---------|---------|
| `src/agent/tools.rs` | 新增函数 | ~60 行 |
| `src/agent/orchestrator.rs` | 新增分支 + Prompt 更新 | ~200 行 |
| `src/agent/coverage.rs` | 模式类别更新 | ~20 行 |
| **总计** | | ~280 行 |
