# TestVDB 交接信息

## 最后更新: 2026-05-14 (系统性改进后)

## 项目状态: 系统性改进完成，实战验证通过

## 当前Git状态
- 分支: main
- 最新commit: 70b090f — feat: VDBFuzz integration + Safety Net architecture fix + 12 new probes
- 系统性改进代码尚未commit（需用户确认后commit）
- cargo test: 68 passed, 0 failed, 1 ignored
- cargo build --release: 成功

## 系统性改进成果

### 改进前后对比

| 指标 | 改进前 | 改进后 | 变化 |
|------|--------|--------|------|
| FA 探索轮次 | 6 turns | 12 turns | +100% |
| Safety Net 执行 | 0 probes | 62 probes (3 batches) | 从0到全覆盖 |
| Oracle 违规发现 | 0 | 8 violations | 从0到8 |
| 缺陷收集 | 1 defect | 17 defects | +1600% |
| API 错误 | N/A | 0 | 无错误 |
| Bug 报告存活断言 | 1个 | 4个 | +300% |

### 6项改进实施

1. **Task 1**: 修复 executor.rs record_test 硬编码参数 → 添加 parse_script_context()
2. **Task 2**: Orchestrator 自动注入 fuzz 结果到初始消息 → FA无需主动调用fuzz工具
3. **Task 3**: 自动注入覆盖率报告到每轮 prompt → FA知道还缺什么
4. **Task 4**: Safety Net 分3批增量执行 (turn 6, turn 10, submit_mre) → 全覆盖
5. **Task 5**: 强化系统 prompt (MANDATORY RULES + 最低轮次检查) → 防止过早提交
6. **Task 6**: 实战验证通过 (4轮迭代修复3个运行时bug)

### 修改的文件

- `src/agent/orchestrator.rs` — 核心改动：prompt重写、fuzz注入、coverage注入、Safety Net分批、submit_mre最低轮次、handle_defect修复
- `src/agent/executor.rs` — 添加 parse_script_context()，修复 record_test
- `src/agent/llm.rs` — 添加 append_content() 方法
- `src/agent/probe.rs` — 修复 label 引号嵌套问题
- `src/target/qdrant.rs` — 修复 Oracle type constraint label

### 修复的运行时 Bug (4轮迭代)

1. DeepSeek API 消息序列错误 → append_content + collected_defects.push()
2. Oracle Python 引号嵌套 → {}=abc 替代 {}='abc'
3. Safety Net sandbox 丢失 → 每次 probe 后 put_sandbox
4. Borrow checker → net.script.clone()

## Bug 报告存活断言 (4个)

1. **hnsw_ef=0** — 接受但文档约束 >= 1
2. **score_threshold=2.0** — 接受但文档约束 0.0-1.0
3. **score_threshold=-0.5** — 接受但文档约束 0.0-1.0
4. **upsert wrong dimension wait=false** — wait=true正确拒绝但wait=false返回200+acknowledged静默丢弃数据

## 已生成的 GitHub Issue 文件

- `issues/qdrant_dimension_off_by_one_65536.md` — off-by-one: FAQ说65535，代码接受65536
- `issues/qdrant_async_upsert_accepts_empty_vector.md` — wait=false跳过维度验证

## 已知限制

1. Safety Net batch 1/2 中部分 Oracle 行为合约脚本因集合名冲突报 Script error（非defect）
2. pip安装偶尔超时导致程序exit code 1
3. NaN/Inf向量无法通过Python requests直接测试（JSON规范不支持）
4. OpenAPI解析器代码已就绪但缺少qdrant_openapi.json
5. 编译有9个warnings（unused imports/fields, dead_code, async_fn_in_trait）

## 关键文件

- `src/agent/orchestrator.rs`: FA编排、Safety Net分批、fuzz注入、coverage注入、prompt
- `src/agent/executor.rs`: 执行器、parse_script_context
- `src/agent/llm.rs`: Message::append_content
- `src/agent/tools.rs`: 工具定义、沙箱执行
- `src/agent/oracle.rs`: Oracle检查引擎
- `src/agent/probe.rs`: 探针生成（修复label引号）
- `src/target/qdrant.rs`: Qdrant特化、36个Safety Net探针、Oracle type constraint
- `src/contract/mod.rs`: 合约解析（parse_constraints_from_assertions）
- `src/agent/vdbfuzz/`: boundary.rs, sequence.rs, coverage.rs
- `contracts/qdrant_behavioral_templates.json`: 45条行为合约模板
- `docs/plans/2026-05-14-vdbfuzz-systematic-improvement.md`: 改进计划（含实施结果）
