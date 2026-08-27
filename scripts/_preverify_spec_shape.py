#!/usr/bin/env python3
"""_preverify_spec_shape.py — D3b spec 对照预验证（C/D 类，v3.4）。

微型类型检查器：脚本的成功路径断言 × spec 声明响应类型 的相容矩阵（C 类），
以及请求体字面 × spec 嵌套必填树的缺失检查（D 类）。与 _classify 的 A/B 类互补，
共用 spec_index（单一 ground truth），产出同 schema 的 script_errors.json 条目
（severity 感知：REJECT 进 retry 工单；WARN 写 preverify_warnings 边车不消耗预算）。

C 类 oracle_shape_conflict（R3 state_02/04 靶标）：
  `b.get("result") is True` → (path="result", kind=identity_bool)
  spec lattice {"result": "object"} → CONFLICT → REJECT
  truthy 断言落在 object → VACUOUS（永真 oracle=不可证伪）→ WARN
D 类 request_required_missing（R3 semantic_004 靶标）：
  body {"points": [{"id":1, "payload":...}]} → 判别键 "points" 消歧 PointsList 分支
  → points[].vector 缺失（PointStruct.required）→ REJECT；anyOf 歧义 → WARN

KISS 边界（防误报优先于召回）：
- 断言只看 safe_request/requests 调用点后 15 条语句内的 If.test/assert/While；
- 响应变量一跳别名（b = resp.json() / b, raw = safe_request(...)）；更远数据流不追；
- spec 未声明的路径不报（服务器可加字段）；
- 请求体含变量/调用求值失败 → 该子树 UNKNOWN 不报。

用法：
    python scripts/_preverify_spec_shape.py <SESSION_DIR> --db qdrant --version v1.18.0
环境：TESTVDB_PREVERIFY=NONE 跳过（legacy 护栏）；TESTVDB_REQUIRED_WARN_ONLY=1
      D 类全降 WARN（校准保险）。
退出码：0=无 REJECT 或跳过；1=存在 REJECT（供 8c 判定走 retry）
"""
from __future__ import annotations

import ast
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import spec_index as si  # noqa: E402

PREVERIFY_VERSION = "D3b-R4.0"
LEGACY_MODE = os.environ.get("TESTVDB_PREVERIFY", "") == "NONE"
HOP_LIMIT = 15

# 断言种类 × 声明类型 相容矩阵（缺省 OK）
#   identity_bool/eq_bool: 期望 boolean；object/array/int/str → CONFLICT
#   truthy: object/array 上永真 → VACUOUS；标量 OK
#   implies_object: 继续 .get/[] → 声明 boolean/int/str → CONFLICT
#   implies_array: 迭代/len/[0] → 声明非 array → CONFLICT
#   key_exists / eq_scalar / isinstance: 全类型 OK（信息不足以冲突）
_COMPAT = {
    "identity_bool": {"object": "CONFLICT", "array": "CONFLICT",
                      "integer": "CONFLICT", "number": "CONFLICT", "string": "CONFLICT"},
    "eq_bool": {"object": "CONFLICT", "array": "CONFLICT",
                "integer": "CONFLICT", "number": "CONFLICT", "string": "CONFLICT"},
    "truthy": {"object": "VACUOUS", "array": "VACUOUS"},
    "implies_object": {"boolean": "CONFLICT", "integer": "CONFLICT",
                       "number": "CONFLICT", "string": "CONFLICT"},
    "implies_array": {"boolean": "CONFLICT", "integer": "CONFLICT", "number": "CONFLICT",
                      "object": "CONFLICT", "string": "CONFLICT"},
}


def _lattice_lookup(lattice: dict, path: str) -> str | None:
    """逐级回退：'result.exists' → 'result.exists' / 'result' / None。

    无点路径（如 'metadata'）不进回退循环——rsplit 对无点串返回原串会死循环
    （alias 展开首次触发该路径形态时实测卡死，2026-08-26 修）。
    """
    if path in lattice:
        return lattice[path]
    if "." not in path:
        return None
    parent = path.rsplit(".", 1)[0].replace("[]", "")
    while parent:
        if parent in lattice:
            return lattice[parent]
        if "." not in parent:
            break
        parent = parent.rsplit(".", 1)[0].replace("[]", "")
    return None


# ---------------------------------------------------------------- 调用点提取

def _iter_http_calls(tree: ast.Module):
    """(method, path_text, call_node)——safe_request / requests.x 两形态；
    path 常量或 f-string 常量段拼接（变量段留空参与模板匹配）。"""
    for n in ast.walk(tree):
        if not isinstance(n, ast.Call):
            continue
        fn = n.func
        name = fn.id if isinstance(fn, ast.Name) else (fn.attr if isinstance(fn, ast.Attribute) else "")
        method, path = None, None
        if name == "safe_request" and len(n.args) >= 2 \
                and isinstance(n.args[0], ast.Constant):
            method = str(n.args[0].value)
            a = n.args[1]
            if isinstance(a, ast.Constant):
                path = str(a.value)
            elif isinstance(a, ast.JoinedStr):
                path = "".join((v.value if isinstance(v, ast.Constant) else "/{v}") for v in a.values)
        elif isinstance(fn, ast.Attribute) and fn.attr in ("get", "post", "put", "delete", "patch") \
                and isinstance(fn.value, ast.Name) and fn.value.id == "requests" and n.args:
            method = fn.attr.upper()
            a = n.args[0]
            if isinstance(a, ast.Constant):
                path = str(a.value)
            elif isinstance(a, ast.JoinedStr):
                path = "".join((v.value if isinstance(v, ast.Constant) else "/{v}") for v in a.values)
        if method and path is not None:
            yield method, path, n


def _resolve_body_literal(call: ast.Call):
    """json=/data= 参数的字面求值（dict/list/scalar）；含 Name/Call → UNKNOWN(None)。"""
    for kw in call.keywords:
        if kw.arg in ("json", "data", "json_body"):
            return _literal(kw.value)
    return None, False


def _literal(node, depth=0):
    """AST 字面求值 → (value, is_unknown)。部分未知保留为 ("__UNKNOWN__", True) 容器。"""
    if depth > 6:
        return ("__UNKNOWN__", True)
    if isinstance(node, ast.Constant):
        return (node.value, False)
    if isinstance(node, ast.Dict):
        out = {}
        unknown = False
        for k, v in zip(node.keys, node.values):
            if not isinstance(k, ast.Constant):
                unknown = True
                continue
            val, unk = _literal(v, depth + 1)
            out[k.value] = val
            unknown = unknown or unk
        return (out, unknown)
    if isinstance(node, ast.List):
        vals, unknown = [], False
        for e in node.elts:
            val, unk = _literal(e, depth + 1)
            vals.append(val)
            unknown = unknown or unk
        return (vals, unknown)
    if isinstance(node, ast.Name):
        return ("__UNKNOWN__", True)
    return ("__UNKNOWN__", True)


# ---------------------------------------------------------------- C 类：断言抽取

def _extract_assertions(expr, var_names: set, prefix=""):
    """布尔表达式 → [(path, kind)]。识别最常见的 6 种形态。"""
    out = []
    if isinstance(expr, ast.UnaryOp) and isinstance(expr.op, ast.Not):
        return _extract_assertions(expr.operand, var_names, prefix)
    if isinstance(expr, ast.BoolOp):
        for v in expr.values:
            out += _extract_assertions(v, var_names, prefix)
        return out
    if isinstance(expr, ast.Compare) and isinstance(expr.left, ast.Call) \
            and isinstance(expr.left.func, ast.Attribute) \
            and expr.left.func.attr == "get" and expr.left.args \
            and isinstance(expr.left.func.value, ast.Name) \
            and expr.left.func.value.id in var_names \
            and isinstance(expr.left.args[0], ast.Constant):
        path = prefix + str(expr.left.args[0].value)
        kind = "key_exists"  # in / is not None / 真值等——信息不足按存在性
        if len(expr.ops) == 1:
            op, comp = expr.ops[0], expr.comparators[0]
            if isinstance(op, (ast.Is, ast.IsNot)) and isinstance(comp, ast.Constant) \
                    and isinstance(comp.value, bool):
                kind = "identity_bool"
            elif isinstance(op, (ast.Eq, ast.NotEq)) and isinstance(comp, ast.Constant) \
                    and isinstance(comp.value, bool):
                kind = "eq_bool"
            elif isinstance(op, (ast.In, ast.NotEq)):
                kind = "key_exists"
            elif isinstance(comp, ast.Constant) and not isinstance(comp.value, bool):
                kind = "eq_scalar"
        # .get("k") 链式再取 .get("j") → implies_object
        if isinstance(expr, ast.Call):
            pass
        out.append((path, kind))
        return out
    # 链式取值：b["result"]["exists"] / b.get("result", {}).get("exists")
    path = _chain_path(expr, var_names, prefix)
    if path is not None:
        out.append((path, "implies_object"))
        return out
    # truthy：直接 if b / if b["k"]
    if isinstance(expr, ast.Subscript):
        p = _chain_path(expr, var_names, prefix)
        if p:
            out.append((p, "truthy"))
    elif isinstance(expr, ast.Name) and expr.id in var_names:
        out.append(("", "truthy"))
    return out


def _chain_path(node, var_names: set, prefix=""):
    """b["a"]["b"] / b.get("a").get("b") → "a.b"（链式取值即 implies_object）。"""
    parts = []
    cur = node
    while True:
        if isinstance(cur, ast.Subscript) and isinstance(cur.slice, ast.Constant) \
                and isinstance(cur.slice.value, str):
            parts.append(cur.slice.value)
            cur = cur.value
        elif isinstance(cur, ast.Call) and isinstance(cur.func, ast.Attribute) \
                and cur.func.attr == "get" and cur.args \
                and isinstance(cur.args[0], ast.Constant) \
                and isinstance(cur.args[0].value, str):
            parts.append(cur.args[0].value)
            cur = cur.func.value
        else:
            break
    if isinstance(cur, ast.Name) and cur.id in var_names and parts:
        return prefix + ".".join(reversed(parts))
    return None


def _collect_get_aliases(tree, bindings) -> dict:
    # 检测边界（R7 实测）：三跳函数封装（body→helper()→result→.get）不追——
    # 该形态（sem02/09）由 builder/auditor 事后层淘汰（R7 四源反证实证）。
    """两跳别名（R7 sem02/09 形态）：`meta = resp.get("k")` 纯取值 Assign →
    {meta_name: (call_id, "k")}——后续 `if meta is None` 可展开回源端点。"""
    aliases: dict[str, tuple[int, str]] = {}
    for n in ast.walk(tree):
        if not isinstance(n, ast.Assign) or len(n.targets) != 1:
            continue
        t = n.targets[0]
        v = n.value
        if not (isinstance(t, ast.Name) and isinstance(v, ast.Call)
                and isinstance(v.func, ast.Attribute) and v.func.attr == "get"
                and v.args and isinstance(v.args[0], ast.Constant)
                and isinstance(v.args[0].value, str)
                and isinstance(v.func.value, ast.Name)
                and v.func.value.id not in ("os", "sys")):
            continue
        src = v.func.value.id
        # 反查 src 绑定的 call（可能多个 call 同名——取全部中任一，位置最近者）
        for cid, names in bindings.items():
            if src in names:
                aliases.setdefault(t.id, (cid, str(v.args[0].value)))
    return aliases


def check_shape_conflicts(tree, index) -> list[dict]:
    """C 类主检查：成功路径断言 × spec lattice。"""
    findings = []
    bindings = _collect_response_bindings(tree)
    aliases = _collect_get_aliases(tree, bindings)
    calls = [c for c in _iter_http_calls(tree)]
    for method, path, call in calls:
        key = si.match_endpoint(method, path, index)
        if not key:
            continue
        entry = index["endpoints"][key]
        lattice = (entry.get("responses", {}).get("200", {}) or {}).get("shape_lattice", {})
        if not lattice:
            continue
        var_names = bindings.get(id(call), set())
        if not var_names:
            continue
        # 调用点后 HOP_LIMIT 行内的布尔上下文
        for n in ast.walk(tree):
            expr = n.test if isinstance(n, (ast.If, ast.While, ast.Assert)) else (
                n.value if isinstance(n, ast.Assign) and isinstance(
                    n.value, (ast.Compare, ast.BoolOp, ast.UnaryOp)) else None)
            if expr is None:
                continue
            if not (0 <= n.lineno - call.lineno <= HOP_LIMIT):
                continue
            assertions = _extract_assertions(expr, var_names)
            # 别名展开：if meta is None（meta = resp.get("k")）→ (k, identity_bool)
            if isinstance(expr, ast.Compare) and isinstance(expr.left, ast.Name)                     and expr.left.id in aliases and aliases[expr.left.id][0] == id(call):
                apath = aliases[expr.left.id][1]
                akind = None
                if len(expr.ops) == 1 and isinstance(expr.comparators[0], ast.Constant):
                    cv = expr.comparators[0].value
                    if isinstance(expr.ops[0], (ast.Is, ast.IsNot)) and cv is None:
                        akind = "key_exists"
                    elif isinstance(expr.ops[0], (ast.Is, ast.IsNot, ast.Eq, ast.NotEq))                             and isinstance(cv, bool):
                        akind = "identity_bool"
                if akind:
                    assertions.append((apath, akind))
            for path_, kind in assertions:
                declared = _lattice_lookup(lattice, path_)
                if declared is None:
                    # 后缀段增强（R7 挂账：sem02/09 顶层路径伪影）：query 路径未声明
                    # 但恰是 lattice 某深路径的尾段（如 "metadata" vs "result.config.metadata"）
                    # → 位置错位信号，WARN（不消耗 retry 预算）
                    leaf = path_.rsplit(".", 1)[-1] if path_ else ""
                    deep = [k for k in lattice if k.endswith("." + leaf) or k == leaf]                         if leaf and len(leaf) > 2 else []
                    if deep and kind in ("identity_bool", "eq_bool", "key_exists", "eq_scalar"):
                        findings.append({"class": "oracle_shape_conflict", "severity": "WARN",
                                         "detail": {"endpoint": key, "path": path_ or "<root>",
                                                    "asserted": kind,
                                                    "reason": "path_suffix_mismatch",
                                                    "declared_paths": deep[:3]}})
                    continue
                if declared in ("any", "recursive", "scalar"):
                    continue
                verdict = _COMPAT.get(kind, {}).get(declared, "OK")
                if verdict == "CONFLICT":
                    findings.append({"class": "oracle_shape_conflict", "severity": "REJECT",
                                     "detail": {"endpoint": key, "path": path_ or "<root>",
                                                "asserted": kind, "declared": declared}})
                elif verdict == "VACUOUS":
                    findings.append({"class": "oracle_shape_conflict", "severity": "WARN",
                                     "detail": {"endpoint": key, "path": path_ or "<root>",
                                                "asserted": kind, "declared": declared,
                                                "reason": "truthy_on_object_cannot_fail"}})
    return findings


def _response_vars(call: ast.Call) -> set:
    """call 赋值目标变量名（b, raw = safe_request(...) / b = resp.json()）。
    元组解包时 qdrant safe_request 返回 (status, body, raw)——body 位是第 2 个。"""
    names = set()
    # 通过父节点关系找赋值——walk 重扫（KISS：扫 Assign 节点比对 value is call）
    return names  # 占位——由调用方传入赋值映射（见 _collect_response_bindings）


def _collect_response_bindings(tree) -> dict:
    """{call_node_id: set(var_name)}——从赋值语句收集。

    覆盖：`b, raw = safe_request(...)`（元组第 2 位）/ `b = safe_request(...)[1]` /
    `b = resp.json()`（resp 同法绑定）。仅一跳。
    """
    bindings: dict[int, set] = {}
    assigns = [n for n in ast.walk(tree) if isinstance(n, ast.Assign)]
    for a in assigns:
        t = a.value
        call = None
        idx = None
        if isinstance(t, ast.Call) and isinstance(t.func, ast.Name) and t.func.id == "safe_request":
            call, idx = t, None
        elif isinstance(t, ast.Call) and isinstance(t.func, ast.Attribute) and t.func.attr == "json":
            inner = t.func.value
            if isinstance(inner, ast.Call) and isinstance(inner.func, ast.Name) \
                    and inner.func.id == "safe_request":
                call, idx = inner, None
        elif isinstance(t, ast.Subscript) and isinstance(t.value, ast.Call) \
                and isinstance(t.value.func, ast.Name) and t.value.func.id == "safe_request" \
                and isinstance(t.slice, ast.Constant):
            call, idx = t.value, t.slice.value
        if call is None:
            continue
        names = set()
        for target in a.targets:
            if isinstance(target, ast.Name):
                names.add(target.id)
            elif isinstance(target, ast.Tuple):
                for i, el in enumerate(target.elts):
                    if isinstance(el, ast.Name) and (idx is None or i == idx or idx == 1):
                        names.add(el.id)
        if names:
            bindings[id(call)] = names
    return bindings


# ---------------------------------------------------------------- D 类：required 树

def check_request_required(tree, index, warn_only=False) -> list[dict]:
    """D 类主检查：请求体字面 × required 树（anyOf 判别键消歧阶梯）。"""
    findings = []
    for method, path, call in _iter_http_calls(tree):
        key = si.match_endpoint(method, path, index)
        if not key:
            continue
        entry = index["endpoints"][key]
        tree_req = (entry.get("request") or {}).get("required_tree")
        if not tree_req:
            continue
        body, unknown = _resolve_body_literal(call)
        if not isinstance(body, dict) or unknown and not body:
            continue
        findings += _diff_required(body, tree_req, key, warn_only)
    return findings


def _diff_required(body: dict, node: dict, endpoint: str, warn_only: bool,
                   prefix: str = "") -> list[dict]:
    out = []
    alts = node.get("alternatives")
    if alts:
        matched = [a for a in alts
                   if (a.get("selector_key") and a["selector_key"] in body)
                   or set(a.get("required") or []) <= set(body.keys())]
        if len(matched) == 1:
            return _diff_required(body, matched[0], endpoint, warn_only, prefix)
        sev = "WARN"  # 歧义/零命中：不消耗 retry 预算
        reason = "ambiguous_branch" if len(matched) > 1 else "no_branch_matched"
        out.append({"class": "request_required_missing", "severity": sev,
                    "detail": {"endpoint": endpoint, "reason": reason, "path": prefix or "<root>"}})
        return out
    for k in node.get("required") or []:
        if k not in body and body.get(k, None) != "__UNKNOWN__":
            out.append({"class": "request_required_missing",
                        "severity": "WARN" if warn_only else "REJECT",
                        "detail": {"endpoint": endpoint, "missing": prefix + k,
                                   "discriminator": node.get("selector_key")}})
    for k, sub in (node.get("children") or {}).items():
        v = body.get(k)
        if isinstance(v, dict):
            out += _diff_required(v, sub, endpoint, warn_only, prefix + k + ".")
        elif isinstance(v, list) and v and isinstance(v[0], dict) and sub.get("items"):
            out += _diff_required(v[0], sub["items"], endpoint, warn_only,
                                  prefix + k + "[].")
    return out


# ---------------------------------------------------------------- 主流程

def _materialize_meta_oracle(f: Path, tree):
    """meta.json 增补第 6 键 oracle（主进程单写者派生，agent 零负担——R2 挂账项落地）。

    docstring Oracle 行原文 + source 标记；oracle_stats.py 优先读 meta（历史回退 docstring）。
    """
    import re as _re
    try:
        doc = ast.get_docstring(tree) or ""
        m = _re.search(r"^\s*Oracle:\s*(.+)$", doc, _re.IGNORECASE | _re.MULTILINE)
        if not m:
            return
        mp = f.parent / (f.stem + ".meta.json")
        if not mp.is_file():
            return
        meta = json.loads(mp.read_text(encoding="utf-8"))
        if "oracle" in meta:
            return  # 已有（幂等）
        meta["oracle"] = {"statement": m.group(1).strip(),
                          "source": "docstring", "preverify_version": PREVERIFY_VERSION}
        mp.write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
    except (OSError, json.JSONDecodeError):
        pass


def scan_session(sd: Path, db: str, version: str, spec_path: str | None = None):
    if LEGACY_MODE:
        print("[preverify] legacy mode (TESTVDB_PREVERIFY=NONE) — skip")
        return 0
    index = si.load_index(db, version, spec_path)
    if index is None:
        print(f"[preverify] spec not found for {db} {version} — skip C/D checks")
        return 0
    script_files = []
    for sub in ("boundary_scripts", "state_scripts", "scripts", "vein_scripts", "debate_logs"):
        d = sd / sub
        if d.is_dir():
            script_files += sorted(d.glob("*.py"))
    script_files += sorted(sd.glob("script_*.py"))
    seen_paths, uniq = set(), []
    for f in script_files:
        rp = str(f.resolve())
        if rp not in seen_paths:
            seen_paths.add(rp)
            uniq.append(f)

    warn_only = os.environ.get("TESTVDB_REQUIRED_WARN_ONLY", "") == "1"
    rejects, warns = [], []
    for f in uniq:
        try:
            tree = ast.parse(f.read_text(encoding="utf-8", errors="replace"), filename=str(f))
        except SyntaxError:
            continue  # syntax 类归 _classify 管
        _materialize_meta_oracle(f, tree)
        shape_findings = check_shape_conflicts(tree, index)
        req_findings = check_request_required(tree, index, warn_only=warn_only)
        for x in shape_findings + req_findings:
            x["script_id"] = f.stem
            (rejects if x["severity"] == "REJECT" else warns).append(x)

    # 写边车 WARN（每脚本一个文件）
    by_script: dict[str, list] = {}
    for w in warns:
        by_script.setdefault(w["script_id"], []).append(w)
    for sid, items in by_script.items():
        p = sd / f"{sid}.preverify_warnings.json"
        p.write_text(json.dumps({"preverify_version": PREVERIFY_VERSION,
                                 "warnings": items}, ensure_ascii=False, indent=1),
                     encoding="utf-8")
    out = {"preverify_version": PREVERIFY_VERSION, "db": db, "version": version,
           "rejects": rejects, "warns": warns}
    (sd / "preverify_findings.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8")
    print(f"[preverify] scripts={len(uniq)} REJECT={len(rejects)} WARN={len(warns)}")
    for r in rejects:
        print(f"  REJECT {r['script_id']}: {r['class']} {json.dumps(r['detail'], ensure_ascii=False)[:120]}")
    return 1 if rejects else 0


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("session_dir")
    ap.add_argument("--db", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--spec", default=None)
    args = ap.parse_args()
    return scan_session(Path(args.session_dir), args.db, args.version, args.spec)


if __name__ == "__main__":
    sys.exit(main())
