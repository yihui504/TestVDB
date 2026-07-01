# Check 协议：VerificationPipeline 接口形式化

**Status**: Accepted (2026-07-01)

## Context

`scripts/verify_live_l1.py` 包含 11 个机械检查函数，覆盖约 90% 的历史假阳性模式。每个检查的函数签名**各不相同**：

- `(log_path)` — 只读日志
- `(candidate, log_path)` — 读缺陷描述 + 日志
- `(candidate, contract)` — 读缺陷描述 + 契约

lambda 调度把它们包装成统一的 `(candidate, log_path)` 接口，但：

1. 真实依赖被闭包隐藏——看函数签名不知道它需要 contract
2. 可选依赖为 None 时的边界条件零处理——`check_constraint_self_violation` 第 43 行在 `contract is None` 时直接崩
3. 新增检查需要三步注册（写函数 + 加 lambda + 加入 checks 列表），没有接口契约约束
4. 这 11 个函数实际上是一个**接缝**处的 11 个**适配器**——但没有形式化的接口定义

## Decision

定义 `Check` 协议，形式化检查接口。创建 `scripts/checks.py`。

### 接口

```python
from dataclasses import dataclass
from typing import Literal, Protocol

@dataclass
class Verdict:
    result: Literal["REFUTED", "UNCERTAIN"]
    reason: str
    check_name: str

@dataclass
class CheckContext:
    contract: dict | None = None   # structured_contract.json，2 个 check 需要
    db_url: str | None = None      # 预留，当前 0 个需要
    target: str = ""               # 目标 DB 标识

class Check(Protocol):
    """单方法接口——所有 L1 检查实现此协议。"""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Verdict | None:
        """返回 Verdict 表示 REFUTED/UNCERTAIN，返回 None 表示不适用（跳过）。"""
        ...
```

### 关键设计约束

1. **`CheckContext` 携带可选依赖**——不需要的 check 忽略对应字段。新增依赖只需改 `CheckContext` 定义，不改 11 个 check 签名。
2. **`check()` 返回 `None` 表示"不适用"**——例如 `ConstraintSelfViolationCheck` 在 `ctx.contract is None` 时返回 `None`（显式处理而非崩溃）。
3. **`Check` 是 Protocol（非 ABC）**——鸭子类型，不需要显式继承。11 个现有函数改为类，各约 3 行改动。
4. **注册机制**——显式列表而非类发现（避免导入副作用，保持检查顺序可控）：

```python
# verify_live_l1.py 中
ALL_CHECKS: list[Check] = [
    PostgresAbortedCheck(),
    ConstraintSelfViolationCheck(),
    ArithmeticCheck(),
    # ... 11 total
]

def verify_l1(candidates, session_dir, contract, target):
    ctx = CheckContext(contract=contract, target=target)
    for check in ALL_CHECKS:
        for candidate in candidates:
            verdict = check.check(candidate, log_path, ctx)
            if verdict and verdict.result == "REFUTED":
                refuted.append(...)
```

### 迁移示例

改造前（`check_constraint_self_violation`）：
```python
def check_constraint_self_violation(candidate, contract):
    desc = candidate.get("description", "")
    # contract 为 None 时直接崩——未处理

checks = [
    ("constraint_self_violation", lambda c, l: check_constraint_self_violation(c, contract)),
]
```

改造后：
```python
class ConstraintSelfViolationCheck:
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Verdict | None:
        if ctx.contract is None:
            return None  # ← 显式处理
        desc = candidate.get("description", "")
        # ... 原有 logic，contract 替换为 ctx.contract
```

## Consequences

- **局部性**：一个 `Check` 协议 = 一处理解检查机制。新增检查 = 实现 `Check` + 加入 `ALL_CHECKS` 列表（2 步，非 3 步）
- **杠杆**：检查流水线（排序、短路、`REFUTED` vs `UNCERTAIN` 收集）对所有 `Check` 适配器可复用
- **可测试性**：每个 `Check` 用 mock `CheckContext` 隔离测试（传入 `contract=None` 验证跳过行为）；流水线行为用 `FakeCheck` 测试
- **接缝质量**：`Check` 协议是真正的接缝——11 个适配器证明接口正确。未来 L2（agent 语义检查）可以实现同一个 `Check` 协议，统一 L1+L2 的调度逻辑
