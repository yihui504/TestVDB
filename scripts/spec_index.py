#!/usr/bin/env python3
"""spec_index.py — OpenAPI spec 确定性索引（D3b 预验证的 ground truth，v3.4）。

从 openapi.json 机械推导两类可判定事实，供预验证（_preverify_spec_shape.py）、
契约物化（enrich_contract_from_spec.py 扩展）与 oracle 校验共用——单一来源，
字节级确定性（dict 排序序列化，同 spec 重建产物可 diff 验证）。

推导内容（per endpoint）：
- request.required_tree：requestBody 递归 $ref/allOf/anyOf/items 解析的必填树
  （ReqNode：type/required/selector_key/children/items/alternatives/nullable/
   depth_capped），派生扁平 required_paths（如 "points[].vector"）
- responses["200"].shape_lattice：成功响应的路径→类型格（如
  {"result": "object", "result.exists": "boolean"}，max_depth 4）

实证驱动（2026-08-26）：
- R3 state_02：exists 端点 lattice 即 {"result":"object","result.exists":"boolean"}
  ——契约 description 写 "200 {result: true|false}" 与 spec 矛盾（description_conflict）
- R3 semantic_004：upsert required_paths 须含 "points[].id"/"points[].vector"
  （PointStruct.required=["id","vector"]，enrich 一级解析覆盖不到的嵌套必填）

防御：深度上限 5 / 每端点节点预算 200（病态 schema 截断不崩溃，depth_capped 标记）；
缓存键 = sha256(spec 文件)，自愈重建。

用法：
    python scripts/spec_index.py --db qdrant --version v1.18.0 [--spec PATH] [--out PATH] [--fresh]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys

MAX_DEPTH = 5
MAX_NODES = 200
LATTICE_DEPTH = 4

# 轻量健康端点（transport_probe_wrong 消费；契约 db_health_endpoints 可覆盖）
HEALTH_DEFAULTS = {"/", "/health", "/healthz", "/livez", "/readyz", "/ready", "/ping", "/status"}

# HTTP helper 名单（bind_endpoint 扫描面——safe_request 权威定义见 _target_api_reference.md）
HELPER_NAMES = {"safe_request", "request", "req"}


class _Budget:
    def __init__(self):
        self.nodes = 0

    def spend(self) -> bool:
        self.nodes += 1
        return self.nodes <= MAX_NODES


def _scalar_type(sch: dict) -> str:
    t = sch.get("type")
    if t:
        return t
    if "properties" in sch or "additionalProperties" in sch:
        return "object"
    if "items" in sch:
        return "array"
    if "enum" in sch:
        return "string"
    return "scalar"


def _deref(sch: dict, schemas: dict):
    """$ref 单跳解引用（返回 (schema, resolved_name)；无 ref 返回原样）。"""
    ref = sch.get("$ref", "")
    if not ref:
        return sch, None
    name = ref.split("/")[-1]
    return schemas.get(name, {}), name


def resolve_reqnode(sch: dict, schemas: dict, seen: frozenset, depth: int, budget: _Budget) -> dict | None:
    """schema → ReqNode（requestBody 侧必填树）。"""
    if depth > MAX_DEPTH or not budget.spend():
        return {"type": "scalar", "required": [], "selector_key": None,
                "children": {}, "items": None, "alternatives": None,
                "nullable": False, "depth_capped": True}
    sch, resolved = _deref(sch, schemas)
    if resolved:
        if resolved in seen:  # 递归 schema：截断
            return {"type": "scalar", "required": [], "selector_key": None,
                    "children": {}, "items": None, "alternatives": None,
                    "nullable": False, "depth_capped": True, "recursive": resolved}
        seen = seen | {resolved}
    if not sch:
        return None
    if "allOf" in sch:
        # 合并：required 并集 + properties 合并（同 OpenAPI 语义）
        merged = {"required": [], "properties": {}}
        for sub in sch["allOf"]:
            s2, r2 = _deref(sub, schemas)
            if r2 and r2 in seen:
                continue
            merged["required"] += s2.get("required") or []
            merged["properties"].update(s2.get("properties") or {})
            merged["required"] += sub.get("required") or []
            merged["properties"].update(sub.get("properties") or {})
        return resolve_reqnode(merged, schemas, seen, depth + 1, budget)
    if "anyOf" in sch or "oneOf" in sch:
        branches = sch.get("anyOf") or sch.get("oneOf") or []
        alts, nullable = [], False
        for sub in branches:
            if sub.get("nullable") or sub.get("type") == "null" or (
                    set(sub.keys()) <= {"nullable"} and sub):
                nullable = True  # anyOf[X, {nullable:true}]：nullable 标记非分支
                continue
            node = resolve_reqnode(sub, schemas, seen, depth + 1, budget)
            if node:
                alts.append(node)
        if not alts:
            return None
        if len(alts) == 1 and nullable:
            alts[0]["nullable"] = True
            return alts[0]
        node = {"type": "object", "required": [], "selector_key": None,
                "children": {}, "items": None, "alternatives": alts,
                "nullable": nullable, "depth_capped": any(a.get("depth_capped") for a in alts)}
        # selector_key：某分支 required 独有键（判别键），供 anyOf 消歧
        all_reqs = [set(a.get("required") or []) for a in alts]
        for i, a in enumerate(alts):
            others = set().union(*(all_reqs[j] for j in range(len(all_reqs)) if j != i)) if len(all_reqs) > 1 else set()
            sel = [k for k in sorted(all_reqs[i] - others)]
            a["selector_key"] = sel[0] if sel else None
        return node
    t = _scalar_type(sch)
    node = {"type": t, "required": list(sch.get("required") or []),
            "selector_key": None, "children": {}, "items": None,
            "alternatives": None, "nullable": bool(sch.get("nullable")), "depth_capped": False}
    if t == "object":
        for k, sub in (sch.get("properties") or {}).items():
            child = resolve_reqnode(sub, schemas, seen, depth + 1, budget)
            if child:
                node["children"][k] = child
    if t == "array" and sch.get("items"):
        it = resolve_reqnode(sch["items"], schemas, seen, depth + 1, budget)
        if it:
            node["items"] = it
    return node


def required_paths(node: dict | None, prefix: str = "") -> list[str]:
    """ReqNode → 扁平必填路径（对象 "." 连接、数组 "[]" 展开；anyOf 只取各分支并集的
    已选路径——预验证消费时按判别键消歧，这里给全量供人读/agent prompt 用）。"""
    if not node:
        return []
    out = []
    for k in node.get("required") or []:
        out.append(prefix + k)
    for k, sub in (node.get("children") or {}).items():
        out += required_paths(sub, prefix + k + ".")
    if node.get("items"):
        out += required_paths(node["items"], prefix.rstrip(".") + "[]" + ".")
    for alt in node.get("alternatives") or []:
        out += required_paths(alt, prefix)
    return sorted(set(out))


def flatten_shape(sch: dict, schemas: dict, prefix: str = "", depth: int = 0,
                  seen: frozenset = frozenset()) -> dict:
    """schema → 路径→类型格（成功响应形状；max_depth 4 截断防爆炸）。

    键格式：对象 "." 连接、数组 "[]"（如 "result.exists" / "points[].vector"）。
    类型词汇 = json schema type（object/array/boolean/integer/number/string）。
    anyOf：取全分支格的并集，类型冲突处记 "any"（保守，COMPAT 矩阵按 any 放行）。
    """
    if depth > LATTICE_DEPTH:
        return {}
    sch, resolved = _deref(sch, schemas)
    if resolved:
        if resolved in seen:
            return {prefix.rstrip("."): "recursive"} if prefix else {}
        seen = seen | {resolved}
    if not sch:
        return {}
    if "anyOf" in sch or "oneOf" in sch:
        merged: dict = {}
        for sub in sch.get("anyOf") or sch.get("oneOf") or []:
            if sub.get("nullable") or sub.get("type") == "null":
                continue
            for k, v in flatten_shape(sub, schemas, prefix, depth, seen).items():
                if k in merged and merged[k] != v:
                    merged[k] = "any"
                else:
                    merged[k] = v
        return merged
    t = _scalar_type(sch)
    out = {}
    if prefix:
        out[prefix[:-1] if prefix.endswith(".") else prefix] = t
    if t == "object":
        for k, sub in (sch.get("properties") or {}).items():
            out.update(flatten_shape(sub, schemas, prefix + k + ".", depth + 1, seen))
    if t == "array" and sch.get("items"):
        out.update(flatten_shape(sch["items"], schemas,
                                 prefix.rstrip(".") + "[]" + ".", depth + 1, seen))
    return out


def _op_request_schema(op: dict):
    """v3: requestBody.content；Swagger 2.0: parameters[in=body].schema。"""
    rb = op.get("requestBody") or {}
    sch = ((rb.get("content") or {}).get("application/json") or {}).get("schema")
    if sch is None:
        for p in op.get("parameters") or []:
            if isinstance(p, dict) and p.get("in") == "body":
                sch = p.get("schema")
                break
    return sch


def _op_response_schema(op: dict):
    """v3: responses.200.content；Swagger 2.0: responses.200.schema。"""
    r200 = (op.get("responses") or {}).get("200") or {}
    sch = ((r200.get("content") or {}).get("application/json") or {}).get("schema")
    if sch is None:
        sch = r200.get("schema")
    return sch


def build_index(openapi_path: str, db: str = "", version: str = "") -> dict:
    raw = open(openapi_path, "rb").read()
    spec = json.loads(raw.decode("utf-8"))
    schemas = spec.get("components", {}).get("schemas", {})
    endpoints = {}
    for path, ops in (spec.get("paths") or {}).items():
        if not isinstance(ops, dict):
            continue
        for method, op in ops.items():
            if method.lower() not in ("get", "put", "post", "delete", "patch"):
                continue
            if not isinstance(op, dict):
                continue
            budget = _Budget()
            entry: dict = {"operation_id": op.get("operationId")}
            req_sch = _op_request_schema(op)
            if req_sch:
                tree = resolve_reqnode(req_sch, schemas, frozenset(), 0, budget)
                entry["request"] = {"required_tree": tree,
                                    "required_paths": required_paths(tree)}
            ok_sch = _op_response_schema(op)
            if ok_sch:
                lattice = flatten_shape(ok_sch, schemas)
                root_t = _scalar_type(_deref(ok_sch, schemas)[0])
                entry["responses"] = {"200": {"shape_lattice": lattice, "root_type": root_t}}
            if entry.get("request") or entry.get("responses"):
                endpoints[f"{method.upper()} {path}"] = entry
    return {"db": db, "version": version,
            "spec_sha256": hashlib.sha256(raw).hexdigest(),
            "endpoints": dict(sorted(endpoints.items()))}


# ---------------------------------------------------------------- 端点绑定

def normalize_path(raw: str) -> str:
    """剥 scheme/host/query → 纯路径（/collections/x?timeout=5 → /collections/x）。"""
    p = raw.strip()
    if "://" in p:
        p = p.split("://", 1)[1]
        p = "/" + p.split("/", 1)[1] if "/" in p else "/"
    p = p.split("?", 1)[0]
    return p if p.startswith("/") else "/" + p


def path_segments(p: str) -> list[str]:
    return [s for s in normalize_path(p).split("/") if s]


def match_endpoint(method: str, raw_path: str, index: dict) -> str | None:
    """脚本原始 path（可含变量/f-string 文本）↔ 索引模板段对齐；字面段多者优先。"""
    if not raw_path:
        return None
    segs = path_segments(raw_path)
    m = (method or "").upper()
    best, best_score = None, -1
    for key in index.get("endpoints", {}):
        km, kp = key.split(" ", 1)
        if km != m:
            continue
        tsegs = [s for s in kp.split("/") if s]
        if len(tsegs) != len(segs):
            continue
        score = sum(1 for t, s in zip(tsegs, segs)
                    if t == s or (t.startswith("{") and t.endswith("}")))
        if score == len(segs) and score > best_score:
            literal = sum(1 for t, s in zip(tsegs, segs) if t == s and not s.startswith("{"))
            best, best_score = key, literal
    return best


def spec_paths_for(db: str, version: str) -> str:
    """约定位置：.sourcedeps/{db}/{version}/openapi.json（相对插件根）。"""
    root = os.environ.get("TESTVDB_PLUGIN_ROOT") or os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(root, ".sourcedeps", db, version, "openapi.json")


def load_index(db: str, version: str, spec_path: str | None = None) -> dict | None:
    """缓存自愈：.cache/spec_index/{db}_{version}.json 且 sha256 匹配 → 直接读。"""
    sp = spec_path or spec_paths_for(db, version)
    if not os.path.isfile(sp):
        return None
    cache = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                         ".cache", "spec_index", f"{db}_{version}.json")
    digest = hashlib.sha256(open(sp, "rb").read()).hexdigest()
    if os.path.isfile(cache):
        try:
            cached = json.load(open(cache, encoding="utf-8"))
            if cached.get("spec_sha256") == digest:
                return cached
        except (OSError, json.JSONDecodeError):
            pass
    idx = build_index(sp, db=db, version=version)
    try:
        os.makedirs(os.path.dirname(cache), exist_ok=True)
        with open(cache, "w", encoding="utf-8", newline="\n") as f:
            json.dump(idx, f, ensure_ascii=False, sort_keys=True, indent=1)
    except OSError:
        pass
    return idx


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--spec", default=None)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    idx = load_index(args.db, args.version, args.spec)
    if idx is None:
        print(f"[spec_index] spec not found: {args.spec or spec_paths_for(args.db, args.version)}")
        return 1
    n = len(idx["endpoints"])
    n_resp = sum(1 for e in idx["endpoints"].values() if e.get("responses"))
    n_req = sum(1 for e in idx["endpoints"].values() if e.get("request"))
    capped = sum(1 for e in idx["endpoints"].values()
                 if (e.get("request") or {}).get("required_tree", {}).get("depth_capped"))
    print(f"[spec_index] {args.db} {args.version}: {n} endpoints "
          f"(request_tree={n_req}, resp_shape={n_resp}, depth_capped={capped})")
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(idx, f, ensure_ascii=False, sort_keys=True, indent=1)
        print(f"[spec_index] written {args.out}")
    # 确定性自检：重建一次比对
    idx2 = build_index(args.spec or spec_paths_for(args.db, args.version),
                       db=args.db, version=args.version)
    ok = json.dumps(idx, sort_keys=True) == json.dumps(idx2, sort_keys=True)
    print(f"[spec_index] determinism: {'OK' if ok else 'MISMATCH'}")
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main())
