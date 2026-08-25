#!/usr/bin/env python3
"""bind_strategies.py — 策略预绑定（v3.4 D2：取消 attack agent 触发规则匹配）

确定性后处理（零 LLM）：读 structured_contract.json + strategy_registry
（global_strategies.json + {target}_strategies.json），为 level=endpoint 的
约束计算 bound_strategies 写回契约。

绑定条件（全部满足才绑）：
  1. strategy.pattern.constraint_types 非空且含约束类型标记（type/range/state，
     容 _constraint 后缀形式）
  2. pattern.applicable_endpoints 与约束 endpoint 匹配：精确相等或通配 `*+<op>`
     （op = endpoint 最后一个 + 段）
  3. status ∈ {active, stable} 且 performance.defects_found ≥ 1
     （experimental / 零战绩策略不自动绑定——防未验证策略污染预绑定）

level=system 约束一律不绑定（走场景构造路径，v3.4 意见 4：系统级宽松覆盖）。
写回：constraints.{type,range,state}_constraints[].bound_strategies = [strategy_id...]
     （无绑定也写空列表——显式表达"已评估"）+ 顶层 _strategy_binding 汇总
     （F 节策略贡献统计的消费入口）。

level lint（fail fast，exit 1）：存在缺 level 字段的约束 → 列清单报错
（契约必须先过 contract-formalizer 规则 2.7 分级）。

用法：
  python scripts/bind_strategies.py <structured_contract.json> [--registry-dir DIR] [--dry-run]
  python scripts/bind_strategies.py --self-check
"""

import copy
import json
import os
import sys
from datetime import datetime, timezone

CLI_EXIT_LINT_FAIL = 1

_GROUPS = (
    ("type_constraints", "type"),
    ("range_constraints", "range"),
    ("state_constraints", "state"),
)

# 内置策略基线（v3.4 D2）：attack agents 规范内建的确定性映射——仅收录
# "策略相对确定、清晰"的两条（导师点名 Boundary Value / Type Boundary，
# 且 attack-boundary.md 规范原文即声明"针对 range_constraints / type_constraints"）。
# state/semantic 内置策略无约束形态级确定映射，不预绑（防"全绑退化"——
# 预绑定的价值在分流，绑一切等于没绑）。builtin: 前缀供 attack agents
# 按内置策略章节直接生成。
BUILTIN_BASELINE = {
    "type": [
        {"strategy_id": "builtin:type_boundary", "agent": "boundary",
         "ref": "attack-boundary.md 策略 2: 类型边界攻击（针对 type_constraints）"},
    ],
    "range": [
        {"strategy_id": "builtin:boundary_value", "agent": "boundary",
         "ref": "attack-boundary.md 策略 1: 边界值攻击（针对 range_constraints）"},
    ],
    "state": [],
}


class LevelMissingError(Exception):
    """契约存在缺 level 字段的约束（规则 2.7 未执行）。"""

    def __init__(self, missing_ids):
        self.missing_ids = missing_ids
        super().__init__(
            "constraints missing `level` (run contract-formalizer rule 2.7 first): "
            + ", ".join(missing_ids))


def _type_tag(constraint_type: str) -> str:
    t = str(constraint_type).strip().lower()
    if t.endswith("_constraint"):
        t = t[: -len("_constraint")]
    return t


def _endpoint_matches(pattern_endpoint: str, constraint_endpoint: str) -> bool:
    """精确相等 / `*` 全匹配 / `*+<op>` 通配（op 匹配任意 resource 段）。"""
    p = str(pattern_endpoint).strip()
    c = str(constraint_endpoint).strip()
    if p == c or p == "*":
        return True
    if p.startswith("*+"):
        op = p[2:]
        return "+" in c and c.rsplit("+", 1)[-1] == op
    return False


def _strategy_binds(strategy: dict, ctype_tag: str, endpoint: str) -> bool:
    pattern = strategy.get("pattern") or {}
    ctypes = [str(t).strip().lower().replace("_constraint", "")
              for t in (pattern.get("constraint_types") or [])]
    if ctype_tag not in ctypes:
        return False
    eps = pattern.get("applicable_endpoints") or []
    if not any(_endpoint_matches(pe, endpoint) for pe in eps):
        return False
    if strategy.get("status") not in ("active", "stable"):
        return False
    perf = strategy.get("performance") or {}
    return int(perf.get("defects_found", 0)) >= 1


def lint_levels(contract: dict) -> list:
    """返回缺 level 字段的 constraint_id 清单（空 = 通过）。"""
    missing = []
    for group, _ in _GROUPS:
        for c in (contract.get("constraints", {}).get(group) or []):
            if not c.get("level"):
                missing.append(c.get("constraint_id", "<no-id>"))
    return missing


def bind_contract(contract: dict, registries: list, registry_names=None) -> dict:
    """纯函数：返回带 bound_strategies / _strategy_binding 的新契约（不改入参）。

    registries 顺序 = 优先级（后传入的同 id 策略覆盖先传入——调用方应
    [global, target] 顺序传入，使 target 特化策略优先）。
    """
    out = copy.deepcopy(contract)
    missing = lint_levels(out)
    if missing:
        raise LevelMissingError(missing)

    by_id = {}
    for reg in registries:
        for s in (reg.get("strategies") or []):
            sid = s.get("strategy_id")
            if sid:
                by_id[sid] = s

    bound = unbound_endpoint = system_skipped = 0
    builtin_bound = registry_bound = 0
    strategies_used = set()
    for group, tag in _GROUPS:
        for c in (out.setdefault("constraints", {}).setdefault(group, []) or []):
            if c.get("level") == "system":
                c["bound_strategies"] = []
                system_skipped += 1
                continue
            builtin = [b["strategy_id"] for b in BUILTIN_BASELINE.get(tag, [])]
            registry = sorted(
                sid for sid, s in by_id.items()
                if _strategy_binds(s, tag, c.get("endpoint", "")))
            sids = sorted(set(builtin) | set(registry))
            c["bound_strategies"] = sids
            if sids:
                bound += 1
                strategies_used.update(sids)
                if builtin:
                    builtin_bound += 1
                if registry:
                    registry_bound += 1
            else:
                unbound_endpoint += 1

    out["_strategy_binding"] = {
        "tool": "bind_strategies.py",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "registry_files": list(registry_names or []),
        "bound_constraints": bound,
        "bound_via_builtin": builtin_bound,
        "bound_via_registry": registry_bound,
        "unbound_endpoint_constraints": unbound_endpoint,
        "system_constraints_skipped": system_skipped,
        "distinct_strategies_bound": len(strategies_used),
    }
    return out


def _load_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _self_check() -> int:
    import tempfile

    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    # ── endpoint 通配匹配 ──
    expect(_endpoint_matches("*+create", "collections+create"), "*+create matches collections+create")
    expect(_endpoint_matches("*+create", "entities+create"), "*+create matches entities+create")
    expect(not _endpoint_matches("*+create", "collections+delete"), "*+create rejects collections+delete")
    expect(not _endpoint_matches("*+create", "create"), "*+create rejects bare op")
    expect(_endpoint_matches("collections+search", "collections+search"), "exact match")
    expect(_endpoint_matches("*", "anything"), "* matches all")
    expect(_type_tag("type_constraint") == "type", "type_tag strips _constraint")
    expect(_type_tag("Range_Constraint") == "range", "type_tag lowercases")

    ok_strategy = {
        "strategy_id": "boundary_type_invalid",
        "status": "active",
        "pattern": {"constraint_types": ["type", "type_constraint"],
                    "applicable_endpoints": ["*+create", "collections+search"]},
        "performance": {"defects_found": 3},
    }
    experimental = {
        "strategy_id": "exp_zero_track",
        "status": "experimental",
        "pattern": {"constraint_types": ["range"],
                    "applicable_endpoints": ["*+create"]},
        "performance": {"defects_found": 0},
    }
    registry = {"strategies": [ok_strategy, experimental]}

    contract = {
        "target": "qdrant",
        "constraints": {
            "type_constraints": [{
                "constraint_id": "qdrant_type_collections_create_001",
                "endpoint": "collections+create", "type": "type_constraint",
                "level": "endpoint",
            }],
            "range_constraints": [{
                "constraint_id": "qdrant_range_entities_search_001",
                "endpoint": "entities+search", "type": "range_constraint",
                "level": "endpoint",
            }],
            "state_constraints": [{
                "constraint_id": "qdrant_state_global_001",
                "endpoint": "collections+delete", "type": "state_constraint",
                "level": "system",
            }],
        },
    }

    # ── lint：缺 level 报错 ──
    broken = copy.deepcopy(contract)
    broken["constraints"]["range_constraints"][0].pop("level")
    try:
        bind_contract(broken, [registry])
        failures.append("missing level should raise LevelMissingError")
    except LevelMissingError as e:
        expect("qdrant_range_entities_search_001" in e.missing_ids,
               "lint lists the offending constraint_id")

    # ── 绑定：builtin 基线 + endpoint 级 registry 匹配 + system 跳过 + experimental 不绑 ──
    bound = bind_contract(contract, [registry], ["global_strategies.json"])
    tc = bound["constraints"]["type_constraints"][0]
    expect(tc["bound_strategies"] == ["boundary_type_invalid", "builtin:type_boundary"],
           f"type constraint binds builtin + registry, got {tc['bound_strategies']}")
    rc = bound["constraints"]["range_constraints"][0]
    expect(rc["bound_strategies"] == ["builtin:boundary_value"],
           f"range constraint binds builtin only (experimental/zero-track blocked), got {rc['bound_strategies']}")
    sc = bound["constraints"]["state_constraints"][0]
    expect(sc["bound_strategies"] == [], "system constraint skipped (no binding)")
    meta = bound["_strategy_binding"]
    expect(meta["bound_constraints"] == 2 and meta["unbound_endpoint_constraints"] == 0
           and meta["system_constraints_skipped"] == 1
           and meta["bound_via_builtin"] == 2 and meta["bound_via_registry"] == 1,
           f"summary counts sane: {meta}")
    expect(contract["constraints"]["type_constraints"][0].get("bound_strategies") is None,
           "input contract not mutated (immutable)")

    # ── CLI 端到端：写文件 → bind → 读回；dry-run 不写 ──
    with tempfile.TemporaryDirectory() as td:
        cpath = os.path.join(td, "structured_contract.json")
        with open(cpath, "w", encoding="utf-8") as f:
            json.dump(contract, f)
        rpath = os.path.join(td, "global_strategies.json")
        with open(rpath, "w", encoding="utf-8") as f:
            json.dump(registry, f)

        # dry-run：不写回
        rc_code = _main(["--registry-dir", td, "--dry-run", cpath])
        expect(rc_code == 0, f"dry-run exit 0, got {rc_code}")
        after = _load_json(cpath)
        expect("bound_strategies" not in after["constraints"]["type_constraints"][0],
               "dry-run must not write")

        # 正式：写回
        rc_code = _main(["--registry-dir", td, cpath])
        expect(rc_code == 0, f"bind exit 0, got {rc_code}")
        after = _load_json(cpath)
        expect(after["constraints"]["type_constraints"][0]["bound_strategies"]
               == ["boundary_type_invalid", "builtin:type_boundary"], "file round-trip binding persisted")
        expect("_strategy_binding" in after, "summary written to file")

        # lint 失败：exit 1
        with open(cpath, "w", encoding="utf-8") as f:
            json.dump(broken, f)
        rc_code = _main(["--registry-dir", td, cpath])
        expect(rc_code == CLI_EXIT_LINT_FAIL, f"lint failure exit 1, got {rc_code}")

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("bind_strategies self-check OK")
    return 0


def _default_registry_dir() -> str:
    return os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                        "strategy_registry")


def _main(argv) -> int:
    registry_dir = _default_registry_dir()
    dry_run = False
    paths = []
    it = iter(argv)
    for a in it:
        if a == "--registry-dir":
            registry_dir = next(it, registry_dir)
        elif a == "--dry-run":
            dry_run = True
        else:
            paths.append(a)
    if not paths:
        print("Usage: python scripts/bind_strategies.py <structured_contract.json> "
              "[--registry-dir DIR] [--dry-run]", file=sys.stderr)
        return CLI_EXIT_LINT_FAIL

    contract_path = paths[0]
    contract = _load_json(contract_path)

    registries, names = [], []
    target = contract.get("target", "")
    for fname in ("global_strategies.json",
                  f"{target}_strategies.json" if target else None):
        if not fname:
            continue
        p = os.path.join(registry_dir, fname)
        if os.path.exists(p):
            registries.append(_load_json(p))
            names.append(fname)

    try:
        out = bind_contract(contract, registries, names)
    except LevelMissingError as e:
        print(f"[bind_strategies] LINT FAIL: {e}", file=sys.stderr)
        return CLI_EXIT_LINT_FAIL

    meta = out["_strategy_binding"]
    print(json.dumps(meta, ensure_ascii=False, indent=2))
    if not dry_run:
        with open(contract_path, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=2)
        print(f"[bind_strategies] written: {contract_path}")
    else:
        print("[bind_strategies] dry-run — no file written")
    return 0


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "--self-check":
        sys.exit(_self_check())
    sys.exit(_main(sys.argv[1:]))


if __name__ == "__main__":
    main()
