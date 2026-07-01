# PipelineState 模块：状态机深化

**Status**: Accepted (2026-07-01)

## Context

流水线状态机逻辑以散文形式隐式分散在 4 个文件中：

- `commands/mine.md` — 阶段枚举 + Step 7 的 30 行内联 JSON 生成 + 每步末尾的 `pipeline_state.json` 更新模板（第 723–761 行）
- `agents/orchestrator.md` — 合法转换规则以注释形式描述
- `scripts/reconstruct_context.py` — 用 `ps.get('phase')` 临时读取
- `scripts/hooks/pipeline_gate.py` — 用临时字段访问检查 `phase != DONE`

合法阶段转换链（`ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → DEBATE_S2 → VERIFY_LIVE → REPORTING → DEFECT_REVIEW → STATE_SAVE → CLEANUP → DONE`）仅存在于 `orchestrator.md` 的注释中，代码层面**零校验**。`mine.md` 中一个笔误就能产生非法状态（如跳过 DEBATE_S1 直接进入 EXECUTION），在流水线中静默传播。

新增一个阶段需要同步修改 4 个文件——典型的**局部性（locality）**失败。

## Decision

抽取 `scripts/pipeline_state.py` 作为 **PipelineState 模块**，采用**模块 + CLI wrapper** 架构：

### 接口（6 个方法 + 2 个属性）

```python
class PipelineState:
    # —— 构造 ——
    @classmethod
    def create(cls, target, version, max_rounds, min_defects, session_dir) -> "PipelineState"
    @classmethod
    def load(cls, session_dir) -> "PipelineState"

    # —— 查询（只读） ——
    @property
    def phase(self) -> str
    @property
    def is_running(self) -> bool
    def summary(self) -> dict  # {round, phase, total_defects, coverage}，供 reconstruct/pipeline_gate 消费

    # —— 状态变更 ——
    def advance(self, to_phase: str, *, phase_data: dict = None) -> None  # 校验 + 写文件
    def mutate(self, **kwargs) -> None  # 白名单计数器更新（current_round, total_defects_confirmed 等）
    def mark_done(self) -> None
```

### 关键设计约束

1. **`phase` 是只读属性**——修改只能通过 `advance()`。`advance()` 在接缝处校验转换合法性，非法调用抛 `InvalidTransition`。
2. **硬编码转换图**——11 个阶段的合法转换关系定义在模块内部（非配置文件）。理由：转换图是系统不变量而非配置项，新增阶段必然伴随 Python 逻辑变更，配置文件不提供实际灵活性。
3. **`mutate()` 只接受白名单字段**——拒绝修改 `phase`、`session_id`、`timestamps` 等不可变字段。计数器（`total_defects_confirmed` 等）只能增加不能减少。
4. **`summary()` 返回稳定小 dict**——调用者不感知内部 13 字段结构。
5. **`create()` 封装 Step 7 的 30 行内联 JSON 生成**——Timestamp 生成、session 目录创建、初始结构写入，一个调用替代 `mine.md` 中的内联 `python -c "..."` 块。

### CLI wrapper

```bash
# mine.md Step 7 替代
python scripts/pipeline_state.py init --target milvus --version v2.4.0 --max-rounds 5 --min-defects 1 --session-dir results/...

# mine.md 每步末尾替代
python scripts/pipeline_state.py advance --session-dir ... --phase DEBATE_S2 --phase-data '{"scripts_generated": 9}'

# mine.md Step 8g 替代
python scripts/pipeline_state.py mutate --session-dir ... --current-round 3 --total-defects 5

# pipeline_gate.py 替代
python scripts/pipeline_state.py status --session-dir ...  # 输出 {phase, is_running, summary}
```

### 消费者迁移

| 当前 | 迁移后 |
|------|--------|
| `commands/mine.md` 内联 `python -c "..."` | `python scripts/pipeline_state.py advance ...` |
| `agents/orchestrator.md` 内联 `python -c "..."` | 同上 |
| `reconstruct_context.py` 临时 dict 访问 | `from pipeline_state import PipelineState; state = PipelineState.load(...)` |
| `pipeline_gate.py` 临时 phase 检查 | `state = PipelineState.load(...); if state.is_running: ...` |

## Consequences

- **局部性**：新增阶段 = 改 1 个文件（`pipeline_state.py` + 转换图），非 4 个
- **杠杆**：非法转换在接缝处被拦截（抛异常），不再静默持久化到 JSON
- **可测试性**：状态机逻辑无需运行完整流水线即可测试；`tests/test_b_side_gates.py` 中现有集成测试成为回归测试
- **数据格式不变**：`pipeline_state.json` v3 schema 保持不变，`advance()` 和 `mutate()` 只是其新的读写入口
- **风险**：如果转换规则更新不及时，`advance()` 会拒绝 mine.md 中合法的阶段转换——这其实是收益（fail-fast），但需要在首次部署时仔细验证
