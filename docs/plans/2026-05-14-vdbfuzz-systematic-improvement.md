# VDBFuzz 系统性改进实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 打破FA"最小努力路径"恶性循环，将设计能力释放度从40%提升到80%+

**Architecture:** 三层改进——(1)强制注入层：orchestrator自动执行fuzz工具并注入结果，不依赖LLM主动调用；(2)状态反馈层：修复state追踪并自动注入覆盖率到prompt；(3)增量验证层：Safety Net分批增量执行而非最后一次性运行

**Tech Stack:** Rust, async/await, serde_json

---

## 核心设计决策

**原则：不依赖LLM的"自觉性"，而是通过系统架构强制执行策略**

FA不调用fuzz工具的根因不是工具不好用，而是LLM天然走"最小努力路径"。解决方案：让orchestrator在FA循环之外自动执行fuzz工具，将结果作为上下文注入，FA只需"选择并执行"而非"决定是否调用"。

---

### Task 1: 修复 executor.rs 的 record_test 硬编码参数

**Files:**
- Modify: `src/agent/executor.rs:229-247`

**问题**: `record_test` 调用时 param_name 硬编码为 "last_test"，endpoint 硬编码为 "unknown"，导致 state.rs 的参数级追踪完全失效。

**方案**: 从测试脚本内容中解析实际操作的 endpoint 和参数名。

- [ ] **Step 1: 添加脚本解析函数**

在 executor.rs 中添加 `parse_script_context` 函数，从 Python 脚本内容中提取 endpoint 和 param 信息：

```rust
fn parse_script_context(code: &str) -> (String, String) {
    let endpoint = if code.contains("/points/search") || code.contains("/points/search") {
        "search"
    } else if code.contains("/points") && (code.contains("upsert") || code.contains("PUT")) {
        "upsert"
    } else if code.contains("/points") && code.contains("DELETE") {
        "delete"
    } else if code.contains("/collections") && (code.contains("PUT") || code.contains("create")) && !code.contains("/points") {
        "create_collection"
    } else if code.contains("/points/scroll") {
        "scroll"
    } else if code.contains("/collections") && code.contains("DELETE") {
        "delete_collection"
    } else if code.contains("/recommend") {
        "recommend"
    } else {
        "unknown"
    };

    let param = if code.contains("hnsw_ef") {
        "hnsw_ef"
    } else if code.contains("score_threshold") {
        "score_threshold"
    } else if code.contains("\"limit\"") || code.contains("'limit'") {
        "limit"
    } else if code.contains("\"offset\"") || code.contains("'offset'") {
        "offset"
    } else if code.contains("\"size\"") || code.contains("'size'") {
        "size"
    } else if code.contains("distance") {
        "distance"
    } else if code.contains("shard_number") {
        "shard_number"
    } else if code.contains("replication_factor") {
        "replication_factor"
    } else if code.contains("oversampling") {
        "oversampling"
    } else if code.contains("exact") {
        "exact"
    } else if code.contains("vector") && (code.contains("[]") || code.contains("NaN") || code.contains("Infinity")) {
        "vector_extreme"
    } else if code.contains("vector") {
        "vector"
    } else if code.contains("count") {
        "count"
    } else if code.contains("payload") {
        "payload"
    } else if code.contains("wait") {
        "wait"
    } else {
        "general"
    };

    (endpoint.to_string(), param.to_string())
}
```

- [ ] **Step 2: 替换 process_result 中的硬编码调用**

将 `process_result` 中三处 `record_test("last_test", "unknown", ...)` 替换为：

```rust
let (endpoint, param) = parse_script_context(code);
// ... 然后在 record_test 调用中使用:
self.state.record_test(&param, &endpoint, result, defect_type);
```

注意：`process_result` 需要接收 `code` 参数。修改函数签名为：
```rust
fn process_result(&mut self, code: &str, output: String, db_url: String) -> anyhow::Result<ExecutionResult>
```

- [ ] **Step 3: 更新所有 process_result 调用点**

在 `execute_test` 和 `execute_in_existing_sandbox_internal` 中传入 `code`。

- [ ] **Step 4: 运行测试验证**

Run: `cargo test`
Expected: 所有现有测试通过

---

### Task 2: Orchestrator 自动注入 fuzz 结果（核心架构变更）

**Files:**
- Modify: `src/agent/orchestrator.rs:146-210`

**问题**: FA从未调用 fuzz_boundary_values 和 fuzz_api_sequence，因为LLM走"最小努力路径"。

**方案**: 在FA循环开始前，orchestrator自动执行fuzz工具，将生成的测试脚本注入初始消息，FA只需选择执行。

- [ ] **Step 1: 在 run() 方法中，FA循环前自动生成fuzz结果**

在 `let mut messages = vec![...]` 之前，添加自动fuzz生成逻辑：

```rust
// Auto-generate boundary value test cases
let boundary_cases = BoundaryValueGenerator::from_contract(&self.contract);
let mut fuzz_context = String::new();

if !boundary_cases.is_empty() {
    fuzz_context.push_str("=== AUTO-GENERATED BOUNDARY VALUE TESTS ===\n");
    fuzz_context.push_str("The following test cases were automatically generated from contract constraints.\n");
    fuzz_context.push_str("PICK ONE and run it with execute_test_script to confirm the defect.\n\n");
    for (i, case) in boundary_cases.iter().enumerate() {
        fuzz_context.push_str(&format!("{}. {} (expected_rejection={})\n", i + 1, case.name, case.expected_rejection));
        let script_preview = if case.script.len() > 600 {
            format!("{}...", &case.script[..600])
        } else {
            case.script.clone()
        };
        fuzz_context.push_str(&format!("   Script:\n   {}\n\n", script_preview.replace('\n', "\n   ")));
    }
}

// Auto-generate API sequence test cases
let sequence_cases = APISequenceExplorer::generate_sequences();
if !sequence_cases.is_empty() {
    fuzz_context.push_str("\n=== AUTO-GENERATED API SEQUENCE TESTS ===\n");
    fuzz_context.push_str("The following multi-step test cases were automatically generated.\n");
    fuzz_context.push_str("PICK ONE and run it with execute_test_script.\n\n");
    for (i, case) in sequence_cases.iter().enumerate() {
        fuzz_context.push_str(&format!("{}. {} [{}] (expected: {:?})\n", i + 1, case.name, case.sequence_type, case.expected_defect));
        let script_preview = if case.script.len() > 600 {
            format!("{}...", &case.script[..600])
        } else {
            case.script.clone()
        };
        fuzz_context.push_str(&format!("   Script:\n   {}\n\n", script_preview.replace('\n', "\n   ")));
    }
}
```

- [ ] **Step 2: 将fuzz结果注入初始用户消息**

修改初始消息，将fuzz结果包含进去：

```rust
let initial_msg = if fuzz_context.is_empty() {
    "Begin exploration. Write a script and use execute_test_script(fresh_sandbox=true) to test it.".to_string()
} else {
    format!(
        "Begin exploration. AUTO-GENERATED test scripts are provided below.\n\
         IMPORTANT: Pick one of the pre-generated scripts and run it with execute_test_script FIRST.\n\
         Then explore further on your own.\n\n{}",
        fuzz_context
    )
};

let mut messages = vec![
    Message::system(system_prompt),
    Message::user(initial_msg),
];
```

- [ ] **Step 3: 同时记录fuzz用例到覆盖率追踪器**

在生成boundary_cases后，记录coverage entries：
```rust
for case in &boundary_cases {
    if let Some((ep, param, val)) = &case.coverage_entry {
        coverage_tracker.record_visit(ep, param, val);
    }
}
```

- [ ] **Step 4: 运行测试验证**

Run: `cargo test`
Expected: 所有现有测试通过

---

### Task 3: 自动注入覆盖率报告到每轮 prompt

**Files:**
- Modify: `src/agent/orchestrator.rs:218-222`

**问题**: 覆盖率数据存在但从不反馈给LLM，FA无法知道"我还没测过什么"。

**方案**: 每轮自动将覆盖率报告注入prompt，无需FA主动调用 get_coverage_report。

- [ ] **Step 1: 在 turn > 0 的状态注入中添加覆盖率报告**

修改 orchestrator.rs 中 turn > 0 的逻辑：

```rust
if turn > 0 {
    let state_json = executor.state.to_prompt_json();
    let coverage_report = coverage_tracker.report();
    let state_msg = format!(
        "=== EXPLORATION STATE ===\n{}\n\n=== COVERAGE REPORT ===\n{}\n\n\
        Based on the state and coverage above, focus on UNTESTED parameters or try a DIFFERENT approach.",
        state_json, coverage_report
    );
    messages.push(Message::user(state_msg));
}
```

- [ ] **Step 2: 运行测试验证**

Run: `cargo test`
Expected: 所有现有测试通过

---

### Task 4: Safety Net 分批增量执行

**Files:**
- Modify: `src/agent/orchestrator.rs:211-585`

**问题**: Safety Net 40+个探针只在最后串行执行，且非multi_defect模式下第一个缺陷就终止。

**方案**: 将Safety Net分为3批，在FA循环的不同阶段增量执行。每批执行后，发现的缺陷注入prompt让FA继续探索。

- [ ] **Step 1: 在FA循环中添加Safety Net增量执行点**

在 orchestrator 的 `run()` 方法中，在 turn 4 和 turn 8 处插入 Safety Net 批次执行：

```rust
// 在 for turn in 0..self.max_turns 循环中添加：
let safety_nets: Vec<_> = self.plugin.safety_nets()
    .into_iter()
    .filter(|net| !net.redundant_with_mutation)
    .collect();
let batch_size = (safety_nets.len() / 3).max(1);

// Turn 4: Run first batch of safety nets
if turn == 4 {
    let batch_end = batch_size.min(safety_nets.len());
    let mut found_in_batch = Vec::new();
    for net in &safety_nets[..batch_end] {
        // execute and collect defects
        ...
    }
    if !found_in_batch.is_empty() {
        messages.push(Message::user(format!(
            "[SAFETY NET] Found {} defect(s) in batch 1: {}. Continue exploring for more.",
            found_in_batch.len(),
            found_in_batch.iter().map(|n| n.clone()).collect::<Vec<_>>().join(", ")
        )));
    }
}

// Turn 8: Run second batch
if turn == 8 {
    let batch_start = batch_size;
    let batch_end = (2 * batch_size).min(safety_nets.len());
    // similar logic
}
```

注意：这需要将 `safety_nets` 的收集提前到循环外，并在循环内按批次执行。

- [ ] **Step 2: 修改submit_mre中的Safety Net逻辑**

submit_mre时只运行剩余未执行的Safety Net批次（第3批），而非全部。

- [ ] **Step 3: 运行测试验证**

Run: `cargo test`
Expected: 所有现有测试通过

---

### Task 5: 强化系统 prompt 策略约束

**Files:**
- Modify: `src/agent/orchestrator.rs:14-43`

**问题**: 系统prompt列出了5步策略但FA不遵循，因为策略描述太抽象。

**方案**: 重写策略部分，使用更强制性的语言和具体的"必须/禁止"规则。

- [ ] **Step 1: 重写 build_system_prompt 中的策略部分**

```rust
fn build_system_prompt(contract_content: &str) -> String {
    format!(
        "You are a security researcher performing Agentic Fuzzing. Find REAL defects where the server violates contracts or silently accepts invalid input.\n\
        \n\
        === TOOLS ===\n\
        `execute_test_script(code, fresh_sandbox?)` — Run Python scripts. Auto-reuses DB across calls. Set fresh_sandbox=true ONLY for clean start.\n\
        `fuzz_boundary_values(focus_params?)` — Auto-generate boundary value tests from contract constraints.\n\
        `fuzz_api_sequence(sequence_type?)` — Auto-generate multi-step API sequence tests.\n\
        `get_coverage_report()` — Show tested vs untested parameters.\n\
        \n\
        === MANDATORY RULES ===\n\
        1. DO NOT submit MRE before turn 5. You MUST explore at least 5 turns first.\n\
        2. DO NOT repeat the same test pattern. Each turn MUST test a DIFFERENT parameter or endpoint.\n\
        3. If AUTO-GENERATED scripts are provided, you MUST execute at least ONE of them before writing your own.\n\
        4. You MUST test at least 3 DIFFERENT parameters before submitting.\n\
        5. After finding a defect, test 2 MORE parameters to see if the same class of defect exists elsewhere.\n\
        \n\
        === EXPLORATION STRATEGY ===\n\
        Turn 1-2: Execute auto-generated boundary/sequence tests from the context above.\n\
        Turn 3-4: Test STATE consistency (upsert N → count=N, delete K → count=N-K).\n\
        Turn 5-6: Test DATA integrity (write → read back → verify match) and ASYNC behavior (wait=true vs wait=false).\n\
        Turn 7+: Test CROSS-STEP lifecycle (create → delete → recreate) and explore untested parameters from coverage report.\n\
        \n\
        === DEFECT TYPES TO LOOK FOR ===\n\
        - ILLEGAL_SUCCESS: Server accepts input that should be rejected (e.g., negative values, zero, out-of-range)\n\
        - POOR_DIAGNOSTICS: Server returns 200 but silently discards data (test wait=true vs wait=false)\n\
        - STATE_VIOLATION: Count mismatch, data inconsistency after operations\n\
        - DATA_CORRUPTION: Write vector → read back → values don't match\n\
        \n\
        === SCRIPT RULES ===\n\
        - Use {{TESTVDB_DB_URL}} as DB URL placeholder\n\
        - time.sleep(0.5) after create, 0.3 after upsert\n\
        - Print [DEFECT: ILLEGAL_SUCCESS|STATE_LOGIC_VIOLATION|DATA_CORRUPTION|POOR_DIAGNOSTICS] on defect\n\
        - sys.exit(1) on defect, sys.exit(0) on pass\n\
        - Unique collection name with uuid\n\
        - Submit with submit_mre when >= 3 surviving assertions found\n\
        \n\
        Contract:\n{}\n",
        contract_content
    )
}
```

- [ ] **Step 2: 在submit_mre中添加最低轮次检查**

在 submit_mre 的 assertion 检查之前添加：

```rust
if turn < 4 {
    messages.push(Message::tool_response(
        &tc.id,
        format!("REJECTED: You must explore for at least 5 turns before submitting (currently turn {}). Continue testing different parameters.", turn + 1),
    ));
    continue;
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test`
Expected: 所有现有测试通过

---

### Task 6: 全量测试 + 实战验证

**Files:**
- None (verification only)

- [ ] **Step 1: 运行 cargo test**

Run: `cargo test`
Expected: 68+ passed, 0 failed

- [ ] **Step 2: 运行 cargo build --release**

Run: `cargo build --release`
Expected: 成功

- [ ] **Step 3: 实战运行**

Run: `cargo run --release -- test --target qdrant --version 1.18.0 --contracts contracts --multi-defect`
Expected: FA应该在前几轮使用auto-generated脚本，探索更多攻击向量

---

## 实施优先级

1. **Task 2** (自动注入fuzz结果) — 最高影响，直接解决FA不用fuzz工具的根因
2. **Task 5** (强化prompt) — 高影响，防止FA过早提交
3. **Task 3** (自动注入覆盖率) — 中高影响，让FA知道还缺什么
4. **Task 1** (修复record_test) — 中影响，让state追踪有意义
5. **Task 4** (Safety Net分批) — 中影响，增加Safety Net发现缺陷的机会
6. **Task 6** (验证) — 必须

---

## 实施结果 ✅ 全部完成

### 实战验证结果 (run_log_final3.txt)

**运行命令**: `cargo run --release -- test --target qdrant --version 1.18.0 --contracts contracts --multi-defect`

**关键指标对比**:

| 指标 | 改进前 (run_log_v7) | 改进后 (run_log_final3) | 变化 |
|------|---------------------|------------------------|------|
| FA 探索轮次 | 6 turns | **12 turns** | +100% |
| Safety Net 执行 | 0 probes | **62 probes (3 batches)** | 从0到全覆盖 |
| Oracle 违规发现 | 0 (无Oracle) | **8 violations** | 从0到8 |
| 缺陷收集 | 1 defect | **17 defects** | +1600% |
| API 错误 | N/A | **0** | 无错误 |
| Gatekeeper | SubmissionGrade | **SubmissionGrade** | 保持 |
| Bug 报告 | 1个缺陷 | **4个存活断言** | 更全面 |

### 改进后运行时间线

1. **Turn 1-5**: FA 使用自动注入的 fuzz 脚本进行探索
2. **Turn 6**: Safety Net batch 1 执行 (probes 0-19)，20个探针全部复用sandbox
3. **Turn 7-9**: FA 继续探索，Oracle 每轮检查6个行为合约
4. **Turn 10**: Safety Net batch 2 执行 (probes 20-39)
5. **Turn 11**: Oracle 发现 8 个违规（offset=0, hnsw_ef=0, score_threshold=0/2/-0.5, vectors.size=0, shard_number=1, hnsw_ef_zero, score_threshold_2）
6. **Turn 12**: FA 提交 MRE，Safety Net batch 3 执行 (probes 40-61)
7. **Gatekeeper**: 17 defects collected, 双重复现验证通过，SubmissionGrade

### Bug 报告存活断言

1. **hnsw_ef=0** — 接受但文档约束 >= 1
2. **score_threshold=2.0** — 接受但文档约束 0.0-1.0
3. **score_threshold=-0.5** — 接受但文档约束 0.0-1.0
4. **upsert wrong dimension wait=false** — wait=true 正确拒绝但 wait=false 返回 200+acknowledged 静默丢弃数据

### 修复的运行时 Bug (4轮迭代)

1. **DeepSeek API 消息序列错误**: Safety Net 消息改用 `append_content`，batch 3 改用 `collected_defects.push()` 替代 `handle_defect!`
2. **Oracle Python 引号嵌套**: `{}='abc'` → `{}=abc`，probe print 语句改用 f-string
3. **Safety Net sandbox 丢失**: 所有4个循环都改为每次 probe 后 `put_sandbox`
4. **Borrow checker**: `net.script` 添加 `.clone()`
