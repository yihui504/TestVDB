#!/usr/bin/env python3
"""Deterministic parameter backfill: structured_contract.json <- OpenAPI spec.

机制化（rerun pilot 2026-08-21 手工 patch 的脚本化，与 Step 4.5 同哲学：
LLM formalizer 对 spec-derived 骨架条目（raw_knowledge.md 的 "Spec-derived
Endpoints" 节）消费不可靠——pilot 实测 65 端点 0 parameters。与其让 LLM 学会读
骨架，不如主进程在 formalizer 之后机械回填，formalizer 规范只声明"spec 端点
参数由脚本兜底，不要编造"。

规则（与 pilot 手工 patch 一致）：
- 只补 parameters 为空的端点（LLM 提取的保留）——除非 --fill-missing-fields
  时对非空端点补 LLM 漏掉的 spec 一级字段（如 create 的 shard_number）
- 端点匹配：契约 path 形如 "points+query" ↔ spec path 后缀
  "/collections/{collection_name}/points/query"；带 / 的契约 path 直接比对
- 字段来源：requestBody $ref schema 的一级 properties + sub_fields（一层嵌套）
  + path/query parameters
- 每个回填字段标 source: "openapi (mechanical backfill)"——不编造约束，
  仅字段名/类型/required（约束提取仍属 formalizer 的文档页职责）
- 回填后 _passport 重签（hash 按规范算法重算）

用法：
    py -3 scripts/enrich_contract_from_spec.py results/{target}/{version} [--spec .sourcedeps/{target}/{version}/openapi.json]

退出码：0 = 回填或无需回填；1 = spec/contract 缺失或解析失败
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


def load_json(p: Path):
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        print(f"[enrich] FAIL parse {p}: {e}", file=sys.stderr)
        return None


def resolve_fields(op: dict, schemas: dict) -> list[dict]:
    """requestBody $ref → 一级 properties（含一层嵌套 sub_fields）。"""
    fields = []
    try:
        sch = op.get("requestBody", {}).get("content", {}).get("application/json", {}).get("schema", {})
        ref = sch.get("$ref", "")
        if not ref:
            return fields
        s = schemas.get(ref.split("/")[-1], {})
        for name, prop in (s.get("properties") or {}).items():
            entry = {"name": name, "type": prop.get("type", "object"),
                     "required": name in (s.get("required") or []),
                     "source": "openapi (mechanical backfill)"}
            sub_ref = prop.get("$ref") or (prop.get("items", {}) or {}).get("$ref", "")
            if sub_ref:
                ss = schemas.get(sub_ref.split("/")[-1], {})
                sub_names = list((ss.get("properties") or {}).keys())[:12]
                if sub_names:
                    entry["sub_fields"] = sub_names
            fields.append(entry)
    except Exception:
        pass
    return fields


def spec_op_for(ep: dict, spec_paths: dict) -> dict | None:
    """契约端点 ↔ spec operation 匹配。

    契约 path 形如 "points+query" / "collections+exists" / "cluster+peer+delete"——
    "+" 是段分隔，末段常带 formalizer 消歧动词后缀（list/get/update/delete），
    spec 里没有（"/collections" GET 就是 list）。匹配顺序：
    1. 原样后缀匹配（points/query → .../points/query）
    2. 去掉末段动词后缀后匹配（collections/list → /collections）
    3. 末段替换为 {param} 占位匹配（cluster/peer/delete → /cluster/peer/{peer_id}）
    """
    needle = ep.get("path", "").replace("+", "/")
    method = str(ep.get("method", "")).lower()
    if not needle:
        return None
    verb_suffixes = ("list", "get", "update", "delete", "create")
    parts = needle.split("/")
    cands = [needle]
    # 去末段动词
    if len(parts) > 1 and parts[-1] in verb_suffixes:
        cands.append("/".join(parts[:-1]))
    # 末段动词 → {param} 占位
    if len(parts) > 1 and parts[-1] in verb_suffixes:
        cands.append("/".join(parts[:-1]) + "/{param}")

    def norm_segments(p: str) -> list[str]:
        return ["{param}" if s.startswith("{") else s for s in p.strip("/").split("/") if s]

    for spath, methods in spec_paths.items():
        if not spath or spath == "/":
            continue
        op = methods.get(method)
        if op is None:
            continue
        # spec 段序列（去 collection 占位前缀，如 /collections/{c}/points/query → [points, query]）
        segs = norm_segments(spath)
        segs_tail = segs[2:] if segs[:2] == ["collections", "{param}"] else segs
        for cand in cands:
            # cand 两侧形态都试：带 collections 首段 / 剥掉后（与 segs_tail 对称）
            cs = norm_segments(cand)
            cs_tail = cs[1:] if (len(cs) > 1 and cs[0] == "collections") else cs
            for cand_segs in (cs, cs_tail):
                if not cand_segs:
                    continue
                if segs_tail == cand_segs or segs_tail[-len(cand_segs):] == cand_segs:
                    return op
                if segs[-len(cand_segs):] == cand_segs:
                    return op
    return None


def enrich(contract: dict, spec: dict, fill_missing_fields: bool = False) -> tuple[dict, int, int]:
    schemas = spec.get("components", {}).get("schemas", {})
    paths = spec.get("paths", {})
    patched_eps = patched_fields = 0
    for ep in contract.get("api_endpoints", []):
        op = spec_op_for(ep, paths)
        if not op:
            continue
        body = resolve_fields(op, schemas)
        pq = [{"name": q.get("name", "?"),
               "type": (q.get("schema") or {}).get("type", "string"),
               "source": "openapi (mechanical backfill)"}
              for q in op.get("parameters", []) if isinstance(q, dict)]
        incoming = pq + body
        if not incoming:
            continue
        existing = [p for p in ep.get("parameters", []) if isinstance(p, dict)]
        if not existing:
            ep["parameters"] = incoming
            patched_eps += 1
            patched_fields += len(incoming)
        elif fill_missing_fields:
            have = {p.get("name") for p in existing}
            add = [f for f in incoming if f.get("name") not in have]
            if add:
                ep["parameters"] = existing + add
                patched_fields += len(add)
    return contract, patched_eps, patched_fields


def rehash(contract: dict) -> None:
    d = {k: v for k, v in contract.items() if k != "_passport"}
    h = hashlib.sha256(json.dumps(d, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()
    if isinstance(contract.get("_passport"), dict):
        contract["_passport"]["contract_hash"] = f"sha256:{h}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("version_dir", help="results/{target}/{version} 目录（含 structured_contract.json）")
    ap.add_argument("--spec", help="openapi.json 路径（默认 {version_dir}/../../.sourcedeps/... 自动探测）")
    ap.add_argument("--fill-missing-fields", action="store_true",
                    help="对 LLM 已提取的端点也补 spec 一级字段（pilot 的 create shard_number 场景）")
    args = ap.parse_args()

    vd = Path(args.version_dir)
    contract_path = vd / "structured_contract.json"
    contract = load_json(contract_path)
    if not contract:
        return 1

    spec_path = Path(args.spec) if args.spec else None
    if spec_path is None:
        # 自动探测：results/{t}/{v} → 逐级向上找 插件根/.sourcedeps/{t}/{v}/openapi.json
        target = contract.get("target", "")
        version = contract.get("version", "")
        for anc in [vd.parent.parent, *vd.parent.parent.parents]:
            cand = anc / ".sourcedeps" / target / version / "openapi.json"
            if cand.exists():
                spec_path = cand
                break
    if not spec_path or not spec_path.exists():
        print("[enrich] OpenAPI spec 不可用，跳过（先跑 fetch_openapi_spec.py）")
        return 0
    spec = load_json(spec_path)
    if not spec:
        return 1

    contract, n_eps, n_fields = enrich(contract, spec, args.fill_missing_fields)
    rehash(contract)
    contract_path.write_text(json.dumps(contract, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[enrich] {n_eps} endpoints patched, {n_fields} fields backfilled "
          f"(fill_missing_fields={args.fill_missing_fields}) -> {contract_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
