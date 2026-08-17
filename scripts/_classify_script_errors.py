#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""_classify_script_errors.py — Stage 1 确定性脚本错误分类（反"attack agent ~25% SCRIPT_ERROR 直接废弃"）。

memory 教训：attack agent 跨 target 反复犯 4 错（status==200 不查 body / rt.request 格式 / cleanup 缺
try-except / 裸 .json() 链式），Stage 1 当前是直接丢弃 → 浪费 + 掩盖有效测试方向。本脚本产 error
分类 + 通用 feedback_hints（规则非答案），让 Orchestrator 重派 attack agent 修后重审（retry 子循环）。

借鉴 pipeline_state._handle_defect_review 的 retry 设计模式（counter + 超限降级）。

5 类静态可检测错误（覆盖 memory 跨 target 4 错里 Stage 1 能识别的）：
  1. syntax_error        — py_compile 失败
  2. bare_json_chain     — AST: requests.X(...).json()["key"] 裸链式（v2.2 已有，纳入统一分类）
  3. safe_request_unused — safe_request 定义但调用计数 0（v2.2 已有，纳入统一分类）
  4. cleanup_unwrapped   — delete/drop/clear 调用未在 try/except 内（attack-boundary/state/semantic 已规范）
  5. verdict_missing     — 无 ^VERDICT: 行（attack-state 强制约束 §3 已要求）

⛔ 红线（呼应 memory 诚实红线）：feedback_hints 是通用规则（"wrap in try/except"），不是 DB 特定
答案（"测 count API exact=false"）。把 qdrant 换 weaviate/milvus 仍合理 = 通用 = 通过。

Usage:
    python scripts/_classify_script_errors.py SESSION_DIR [SESSION_DIR ...]
    （扫描 SESSION_DIR/{boundary,state,scripts}_scripts/ 下所有 *.py）
Exit:
    0 = no errors (PASS), 1 = errors found (FAIL), 2 = usage/error
"""
from __future__ import annotations

import ast
import datetime
import json
import py_compile
import re
import sys
from pathlib import Path

# Windows cp1252 stdout 兼容（attack-boundary 模板同款）
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# ---------------- 常量 ----------------

# cleanup 必须包 try/except 的 teardown 调用名（attack-boundary/state/semantic 已规范的统一集合）
TEARDOWN_NAMES = {
    "delete_collection", "delete_collections", "delete", "drop",
    "drop_collection", "drop_collections", "drop_schema", "clear",
    "remove", "remove_all",
}

# VERDICT 行：实际 attack agent 都用 `print("VERDICT: X")` 形态（attack-boundary/state/semantic
# 模板都是 print 字符串）。检测源码里是否含 `VERDICT: <X>` 字面量（任意位置——print 内、f-string
# 内、裸语句都算；AST 检测 print 字符串太脆，KISS 用文本扫描）。
VERDICT_RE = re.compile(r"VERDICT:\s+(DEFECT_FOUND|NO_DEFECT|SCRIPT_ERROR)\b")

# feedback_hints：通用规则（非 DB 特定答案），按 error_class
FEEDBACK_HINTS: dict[str, str] = {
    "syntax_error": (
        "Python 编译失败 (py_compile). Read the SyntaxError line/offset from "
        "script_errors.json and fix only that line — do not rewrite the whole script."
    ),
    "bare_json_chain": (
        "Replace `requests.X(...).json()['key']` chains with the three-tuple "
        "`status, body, raw = safe_request(...)` unpacking (see agents/_target_api_reference.md). "
        "Bare .json() crashes on non-JSON responses → guaranteed SCRIPT_ERROR."
    ),
    "safe_request_unused": (
        "safe_request is defined but never called. Route every HTTP call through "
        "safe_request(...); delete the dead wrapper or wire it in."
    ),
    "cleanup_unwrapped": (
        "Teardown calls ({names}) must be wrapped in `try: ... except Exception: pass` — "
        "cleanup failure must not cause non-zero exit (hides the real VERDICT)."
    ).format(names=", ".join(sorted(TEARDOWN_NAMES))),
    "verdict_missing": (
        "Script has no line matching `^VERDICT: <DEFECT_FOUND|NO_DEFECT|SCRIPT_ERROR>$`. "
        "Add exactly one VERDICT line at script end (in `finally:` if wrapped in try/except)."
    ),
}

# ---------------- AST 检测 ----------------

def _is_bare_json_chain(node: ast.AST) -> bool:
    """检测 requests.X(...).json()["key"] 或 .json()["k1"]["k2"] 裸链式。

    模式：Subscript(Call(Attribute(Call(...), 'json'), ...))
    即一个 Subscript 索引一个 .json() 调用结果。
    """
    # Subscript.value = Call(func=Attribute(attr='json', value=Call(func=Name('requests')/Attribute)))
    if not isinstance(node, ast.Subscript):
        return False
    call = node.value
    if not isinstance(call, ast.Call):
        return False
    func = call.func
    if not isinstance(func, ast.Attribute) or func.attr != "json":
        return False
    # inner call: requests.X(...) 或 some_client.X(...)
    inner = func.value
    return isinstance(inner, ast.Call)


def _collect_unwrapped_teardowns(tree: ast.AST) -> list[tuple[int, str]]:
    """返回 [(lineno, label)] — 未在任何 except handler 保护下的 teardown 调用。

    两种 teardown 形态都检测（attack agent 实战两种都有）：
      a) 直接 name 调用：delete_collection(...) / rt.drop_schema(...) — name ∈ TEARDOWN_NAMES
      b) safe_request 包装：safe_request("DELETE", ...) / safe_request("DROP", ...)
         — safe_request 第一个位置参数是字符串 ∈ {"DELETE","DROP","CLEAR","REMOVE"}

    保护判定：向上回溯 parent 链，找到 enclosing Try 且 node 在 try.body（非 orelse/finalbody）
    且 try.handlers 非空。
    """
    # 建 parent map（id(child) → parent）
    parents: dict[int, ast.AST] = {}
    for parent in ast.walk(tree):
        for child in ast.iter_child_nodes(parent):
            parents[id(child)] = parent

    HTTP_TEARDOWN_METHODS = {"DELETE", "DROP", "CLEAR", "REMOVE"}

    def _is_teardown_call(node: ast.Call) -> str | None:
        """返回 teardown label（如 'delete_collection' / 'DELETE'）或 None。"""
        f = node.func
        # 形态 a: 直接 name 调用
        name = None
        if isinstance(f, ast.Name):
            name = f.id
        elif isinstance(f, ast.Attribute):
            name = f.attr
        if name in TEARDOWN_NAMES:
            return name
        # 形态 b: safe_request("DELETE", ...) — func name 是 safe_request + 第一参数字符串
        sfname = None
        if isinstance(f, ast.Name):
            sfname = f.id
        elif isinstance(f, ast.Attribute):
            sfname = f.attr
        if sfname == "safe_request" and node.args:
            first = node.args[0]
            if isinstance(first, ast.Constant) and isinstance(first.value, str):
                if first.value.upper() in HTTP_TEARDOWN_METHODS:
                    return first.value.upper()
        return None

    unwrapped: list[tuple[int, str]] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        label = _is_teardown_call(node)
        if label is None:
            continue
        # 冒烟实证（chroma 2026-08-17）：setup 预清理（建 collection 前删同名残留的
        # safe_request("DELETE", ...)）与 teardown 清理语义不同——前者失败无害（本来
        # 就可能不存在），不需要 try 保护。区分法：同一行/紧邻下一语句是创建类调用
        # （POST/PUT/create_*/add）→ setup 预清理，放行。
        if _is_setup_preardown(tree, parents, node):
            continue
        # 向上找 enclosing Try（必须 handlers 非空 + node 在 try.body）
        cur: ast.AST | None = parents.get(id(node))
        protected = False
        while cur is not None:
            if isinstance(cur, ast.Try) and cur.handlers and _node_in_body(node, cur):
                protected = True
                break
            cur = parents.get(id(cur))
        if not protected:
            unwrapped.append((getattr(node, "lineno", 0), label))
    return unwrapped


def _is_setup_preardown(
    tree: ast.AST,
    parents: dict[int, ast.AST],
    node: ast.Call,
) -> bool:
    """node 是否为 setup 预清理（冒烟实证的误报源，2026-08-17）。

    判据：teardown 调用所在的**语句级祖先**的下一兄弟语句（Module/If/For 体里同层）
    是创建类调用 → 判 setup 预清理。覆盖两种典型布局：
      safe_request("DELETE", col)   ← 本节点（setup 预清理）
      status = safe_request("POST", col)  ← 下一语句创建
    或：
      client.delete_collection(col)
      col = client.create_collection(...)
    """
    # 1. 找 node 的语句级祖先（最近 Expr/Assign 祖先）
    stmt = node
    chain = [node]
    cur = parents.get(id(node))
    while cur is not None:
        chain.append(cur)
        if isinstance(cur, (ast.Expr, ast.Assign, ast.AugAssign)):
            stmt = cur
            break
        cur = parents.get(id(cur))
    else:
        stmt = chain[-1] if chain else node
    # 兜底：向上再爬到最近的 stmt 节点
    s = stmt if isinstance(stmt, ast.stmt) else node
    # 2. 找 s 在哪个 body 里 + 它的下一兄弟
    parent_of_s = parents.get(id(s))
    if parent_of_s is None:
        return False
    body = None
    for attr in ("body", "orelse", "finalbody"):
        v = getattr(parent_of_s, attr, None)
        if isinstance(v, list) and s in v:
            body = v
            break
    if body is None:
        return False
    idx = body.index(s)
    if idx + 1 >= len(body):
        return False
    nxt = body[idx + 1]
    # 3. 下一兄弟是创建类调用？ 取它的最外层 Call（Expr(call) / Assign(value=call) / 直接 call）
    call = None
    if isinstance(nxt, ast.Expr) and isinstance(nxt.value, ast.Call):
        call = nxt.value
    elif isinstance(nxt, ast.Assign) and isinstance(nxt.value, ast.Call):
        call = nxt.value
    elif isinstance(nxt, ast.Call):
        call = nxt
    if call is None:
        return False
    f = call.func
    cname = f.attr if isinstance(f, ast.Attribute) else (f.id if isinstance(f, ast.Name) else None)
    if cname in ("safe_request", "request") and call.args:
        first = call.args[0]
        if isinstance(first, ast.Constant) and isinstance(first.value, str):
            return first.value.upper() in ("POST", "PUT", "PATCH")
    return cname is not None and (
        cname.startswith("create_") or cname.startswith("add") or cname in ("upsert", "insert")
    )


def _node_in_body(target: ast.AST, try_node: ast.Try) -> bool:
    """target 是否在 try_node.body 列表里（直接或嵌套）。"""
    for child in try_node.body:
        if target is child:
            return True
    # 嵌套：target 是 body 里某个节点的后代
    for child in try_node.body:
        for descendant in ast.walk(child):
            if descendant is target:
                return True
    return False


def _safe_request_call_count(tree: ast.AST) -> int:
    """统计 safe_request(...) 调用次数（任一 Name/Attribute 调用名为 safe_request）。"""
    count = 0
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        f = node.func
        if isinstance(f, ast.Name) and f.id == "safe_request":
            count += 1
        elif isinstance(f, ast.Attribute) and f.attr == "safe_request":
            count += 1
    return count


def _safe_request_defined(tree: ast.AST) -> bool:
    """检测 safe_request 是否被定义（FunctionDef / Assign）。"""
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "safe_request":
            return True
        if isinstance(node, ast.Assign):
            for t in node.targets:
                if isinstance(t, ast.Name) and t.id == "safe_request":
                    return True
    return False


# ---------------- 单脚本分类 ----------------

def classify_script(script_path: Path) -> dict | None:
    """返回 {script_id, script_path, error_classes, feedback_hints} 或 None（无法读取）。

    一个脚本可命中多类错误，全部列出。
    """
    try:
        src = script_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None

    sid = script_path.stem
    error_classes: list[str] = []

    # 1. syntax check (py_compile)
    try:
        py_compile.compile(str(script_path), doraise=True)
    except py_compile.PyCompileError as e:
        error_classes.append("syntax_error")
        # 语法错 → AST 解析会失败，后面跳过
        return _build_entry(sid, script_path, error_classes)
    except OSError:
        error_classes.append("syntax_error")
        return _build_entry(sid, script_path, error_classes)

    # AST 解析（语法已通过 py_compile，这里应成功）
    try:
        tree = ast.parse(src, filename=str(script_path))
    except SyntaxError:
        error_classes.append("syntax_error")
        return _build_entry(sid, script_path, error_classes)

    # 2. bare .json() chain
    for node in ast.walk(tree):
        if _is_bare_json_chain(node):
            error_classes.append("bare_json_chain")
            break

    # 3. safe_request defined but unused
    if _safe_request_defined(tree) and _safe_request_call_count(tree) == 0:
        error_classes.append("safe_request_unused")

    # 4. teardown unwrapped
    if _collect_unwrapped_teardowns(tree):
        error_classes.append("cleanup_unwrapped")

    # 5. verdict line missing
    if not VERDICT_RE.search(src):
        error_classes.append("verdict_missing")

    if not error_classes:
        return None
    return _build_entry(sid, script_path, error_classes)


def _build_entry(sid: str, path: Path, classes: list[str]) -> dict:
    # 去重保序
    seen: set[str] = set()
    unique: list[str] = []
    for c in classes:
        if c not in seen:
            seen.add(c)
            unique.append(c)
    return {
        "script_id": sid,
        "script_path": str(path),
        "error_classes": unique,
        "feedback_hints": {c: FEEDBACK_HINTS[c] for c in unique if c in FEEDBACK_HINTS},
    }


# ---------------- 主入口 ----------------

def _scan_session_dir(session_dir: Path) -> list[Path]:
    """扫描 session_dir 下 {boundary,state,scripts}_scripts/ + debate_logs/ + 根 script_*.py 兜底。

    冒烟实证（2026-08-17）：attack agents 按 agent 规范把脚本写 debate_logs/（与 .meta.json
    同目录），而本分类器此前只扫 *_scripts/ 子目录 → Total scripts: 0 的漏检。
    debate_logs/ 加入扫描面（与 *_scripts/ 内容重复时由去重逻辑按 resolve() 归并）。
    """
    scripts: list[Path] = []
    for sub in ("boundary_scripts", "state_scripts", "scripts", "vein_scripts", "debate_logs"):
        d = session_dir / sub
        if d.is_dir():
            scripts.extend(sorted(d.glob("*.py")))
    # 兜底：根目录 script_*.py
    scripts.extend(sorted(session_dir.glob("script_*.py")))
    # 去重（保持顺序）：路径去重 + 文件名去重（冒烟实证：脚本会在 debate_logs/ 与
    # *_scripts/ 双目录同名共存，只按 resolve() 不去重 → 同一脚本被分类两次、
    # retry counter 双计。同名文件内容一致，按文件名归并即可）
    seen: set[Path] = set()
    seen_names: set[str] = set()
    out: list[Path] = []
    for p in scripts:
        rp = p.resolve()
        if rp in seen or p.name in seen_names:
            continue
        seen.add(rp)
        seen_names.add(p.name)
        out.append(p)
    return out


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: _classify_script_errors.py SESSION_DIR [SESSION_DIR ...]", file=sys.stderr)
        return 2

    all_errors: list[dict] = []
    total_scripts = 0
    sessions: list[str] = []

    for sd_arg in sys.argv[1:]:
        sd = Path(sd_arg)
        if not sd.is_dir():
            print(f"WARN: {sd} not a dir, skip", file=sys.stderr)
            continue
        sessions.append(str(sd))
        for script in _scan_session_dir(sd):
            total_scripts += 1
            entry = classify_script(script)
            if entry:
                all_errors.append(entry)

    # 写 report 到第一个 session_dir（多 session 时主进程合并）
    out_dir = Path(sys.argv[1])
    report = {
        "scanned_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "sessions_scanned": sessions,
        "total_scripts": total_scripts,
        "total_errors": len(all_errors),
        "errors": all_errors,
        "verdict": "FAIL" if all_errors else "PASS",
    }
    out_path = out_dir / "script_errors.json"
    out_path.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

    # 摘要打印
    print("=== Script Error Classification ===")
    print(f"Sessions scanned: {len(sessions)}")
    print(f"Total scripts: {total_scripts}")
    print(f"Errors found: {len(all_errors)}")
    if all_errors:
        # 按 error_class 聚合统计
        class_count: dict[str, int] = {}
        for e in all_errors:
            for c in e["error_classes"]:
                class_count[c] = class_count.get(c, 0) + 1
        print("\nError class breakdown:")
        for c, n in sorted(class_count.items(), key=lambda kv: -kv[1]):
            print(f"  {c}: {n}")
        print(f"\nTop 10 error scripts:")
        for e in all_errors[:10]:
            print(f"  ⚠️  {e['script_id']}: {', '.join(e['error_classes'])}")
    print(f"\nverdict: {report['verdict']}")
    print(f"report: {out_path}")
    return 0 if not all_errors else 1


if __name__ == "__main__":
    sys.exit(main())
