# DebateRecord 模块：final_verdict.json schema 拥有者

**Status**: Accepted (2026-07-01)

## Context

`final_verdict.json`（ADR-0002 定义的"唯一事实源"）被 3 个消费者读取：

- `reporter` — 获取 endorsement=true 的缺陷列表，生成 defect-N.md
- `reporter-mre` — 获取缺陷信息，生成自包含 MRE 脚本
- `reconstruct_context.py` — 获取总缺陷数、覆盖率等进度摘要

当前三个消费者都用原始 dict 访问：

```python
d.get("defect_id", "")
d.get("endorsement", False)
d.get("gate_grade", "UNKNOWN")
```

字段拼写错误在运行时静默返回默认值——这在 reporter（人工审查入口）尤其危险。

同时 `stage2_aggregation.json` 被 7 个消费者读取，但各消费者对字段的需求不同（`verify_live_l1.py` 关心 `script`+`description`，`novelty_gate.py` 关心 `script`+`param`+`defect_type`），强行统一会造成"所有人的字段合集"——又一个浅模块。因此**暂不纳入**，等 per-judge 产出格式稳定后再议。

## Decision

创建 `scripts/debate_record.py` 作为 **DebateRecord 模块**，首期范围限定为 `final_verdict.json`。

### 接口

```python
@dataclass
class DefectVerdict:
    defect_id: str
    script: str
    param: str
    param_name: str
    defect_type: str
    judge_doc: str
    judge_evidence: str
    judge_novelty: str
    judge_severity: str
    gate_grade: str
    gate_layer: str
    gate_evidence_url: str
    endorsement: bool
    endorsement_reason: str
    judge_discrepancy: bool

@dataclass
class FinalVerdict:
    generated_at: str
    session_dir: str
    total_defects: int
    defects: list[DefectVerdict]

    @classmethod
    def from_file(cls, session_dir: str | Path) -> "FinalVerdict"
    def endorsed(self) -> list[DefectVerdict]  # endorsement == True
    def rejected(self) -> list[DefectVerdict]   # endorsement == False
    def summary(self) -> dict                   # {total, endorsed, rejected, grades: {...}}
```

### 关键设计约束

1. **Schema 校验在加载时完成**——`from_file()` 校验所有字段存在且类型正确，校验失败抛 `SchemaValidationError`（含缺失字段名和期望类型）。消费者不再需要防御性 `get(..., default)`。
2. **`endorsed()` / `rejected()` 封装过滤逻辑**——消费者不自己写 `[d for d in v.defects if d.endorsement]`。如果未来 endorsement 判定逻辑变化（如增加新的 Gate 分级），改一处即可。
3. **`summary()` 返回稳定摘要**——`reconstruct_context.py` 不需要知道内部结构。
4. **版本字段**——`FinalVerdict` 含隐式 schema version（当前 v1），未来格式演进时 `from_file()` 可做版本迁移。

### 消费者迁移

| 当前 | 迁移后 |
|------|--------|
| `reporter`：`d.get("endorsement", False)` | `v.endorsed()` 返回类型化列表 |
| `reporter-mre`：`d.get("defect_id", "")` | `defect.defect_id`（类型安全） |
| `reconstruct_context.py`：`data.get("total_defects", 0)` | `v.summary()["total"]` |

## Consequences

- **局部性**：`final_verdict.json` 格式变更 = 改 1 个文件（`debate_record.py`），消费者通过类型化访问自动获得变更
- **杠杆**：3 个消费者从 1 个模块获得 schema 校验 + 类型安全 + 过滤方法
- **可测试性**：`from_file()` 的 schema 校验可隔离测试；消费者测试可用 mock `FinalVerdict`
- **删除测试**：删掉此模块会使格式知识散落回 3 个消费者——它在挣口粮
- **范围约束**：`stage2_aggregation.json` 暂不纳入（7 个消费者需求不同，强制统一会制造浅模块）。等 per-judge 产出格式稳定后作为 v2 扩展
