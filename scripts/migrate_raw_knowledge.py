#!/usr/bin/env python3
"""migrate_raw_knowledge.py — raw_knowledge.md → raw_knowledge.json 一次性迁移（v3.4 §A）

策略：**浅层结构化 + raw_block 保真**。
- 确定解析：章节切分、端点的 method/path/source_url/doc_version、Document Metadata
  键值、Document Sources 表格行、SDK/Docker（→ deployment_meta.json）。
- 原文保真：端点的参数/约束/响应细节整块进 `raw_block`——深层 markdown 嵌套解析
  易碎且无必要（消费方 contract-formalizer 是 LLM，读结构化入口 + 原文块即可）；
  机器消费方（validate_doc_coverage）只需 method/path + 全文 token。
- 非模板章节（Version-Gated Features / Inter-Document Conflicts / Concept-Doc
  Minimum 等）→ `sections[]`（title + content 原文），不丢内容。

产出：同目录 raw_knowledge.json + deployment_meta.json（原 .md 保留不动，回滚兜底）。

用法：
  python scripts/migrate_raw_knowledge.py <dir-containing-raw_knowledge.md> [--dry-run]
  python scripts/migrate_raw_knowledge.py --self-check
"""

from __future__ import annotations

import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

_METHOD_PATH = re.compile(
    r"-\s*Method:\s*(GET|POST|PUT|PATCH|DELETE|SQL)\s*[—-]{1,2}\s*Path:\s*(\S+)", re.I)
_METHOD_LINE = re.compile(r"-\s*Method:\s*(GET|POST|PUT|PATCH|DELETE|SQL)\s*$", re.I)
_PATH_LINE = re.compile(r"-\s*Path:\s*(\S+)")
# 压缩格式：method/path 嵌在 #### 标题行（真实 md 两种形态）
_H4_MP = re.compile(
    r"^####\s+(.+?)\s+—\s*Method:\s*(GET|POST|PUT|PATCH|DELETE|SQL)\s+—\s*Path:\s*(\S+)", re.I)
_H4_SHORT = re.compile(
    r"^####\s+(.+?)\s+—\s*(GET|POST|PUT|PATCH|DELETE|SQL)\s+(/\S*)", re.I)
_SOURCE_URL = re.compile(r"Source URL:\s*(\S+)|Source:\s*(\S+\.md)")
_DOC_VERSION = re.compile(r"Doc Version:\s*(\S+)")
_KV = re.compile(r"^-\s*([A-Za-z_][\w ]*?):\s*(.+)$")
_TABLE_ROW = re.compile(r"^\|\s*\d+\s*\|")
_TITLE = re.compile(r"^#\s+(.+?)\s+([\w.]+)\s+API Knowledge", re.I)


def _split_sections(md: str):
    """按 ## 标题切分，返回 [(title, body)]。首块（# 标题前）丢弃。"""
    parts = re.split(r"^## ", md, flags=re.MULTILINE)
    out = []
    for p in parts[1:]:
        lines = p.split("\n", 1)
        title = lines[0].strip()
        body = lines[1] if len(lines) > 1 else ""
        out.append((title, body))
    return out


def _parse_metadata(body: str) -> dict:
    meta = {}
    for ln in body.split("\n"):
        m = _KV.match(ln.strip())
        if m:
            meta[m.group(1).strip().lower().replace(" ", "_")] = m.group(2).strip()
    return meta


def _parse_sources(body: str) -> list:
    rows = []
    for ln in body.split("\n"):
        s = ln.strip()
        if _TABLE_ROW.match(s):
            cells = [c.strip() for c in s.strip("|").split("|")]
            if len(cells) >= 5:
                rows.append({"url": cells[1], "doc_version": cells[2],
                             "fetched_at": cells[3], "version_match": cells[4],
                             "kind": "unknown"})
    return rows


def _parse_deployment(sdk_body: str, docker_body: str) -> dict:
    def kvs(body):
        d = {}
        for ln in body.split("\n"):
            m = _KV.match(ln.strip())
            if m:
                d[m.group(1).strip().lower().replace(" ", "_")] = m.group(2).strip()
        return d
    sdk = kvs(sdk_body)
    dock = kvs(docker_body)
    return {
        "sdk": {"package": sdk.get("package", ""), "version": sdk.get("version", ""),
                "install": sdk.get("install", "")},
        "docker_images": {"available_tags": dock.get("available_tags", ""),
                          "recommended": dock.get("recommended", "")},
    }


def _parse_endpoints(body: str):
    """#### 端点块 → 浅层结构 + raw_block。### 三级标题（容缩进）作 category 分组。

    返回 (endpoints, category_notes)：category 间的散行（如 Snapshots 批量列表段，
    无 #### 子标题）累积进 category_notes[category]，不丢内容。
    """
    eps = []
    category_notes = {}
    category = None
    current = None
    pending = []

    def flush():
        nonlocal current
        if current:
            current["raw_block"] = current.pop("_raw").strip()
            eps.append(current)
            current = None

    def flush_pending():
        nonlocal pending
        text = "\n".join(pending).strip()
        if text and category:
            category_notes.setdefault(category, "")
            category_notes[category] = (category_notes[category] + "\n" + text).strip()
        pending = []

    for ln in body.split("\n"):
        h3 = re.match(r"^\s*###\s+(.+)$", ln)
        if h3 and not re.match(r"^####", ln):
            flush()
            flush_pending()
            category = h3.group(1).strip()
            continue
        h4 = re.match(r"^####\s+(.+)$", ln)
        if h4:
            flush()
            flush_pending()
            title = h4.group(1).strip()
            m = _H4_MP.match(ln) or _H4_SHORT.match(ln)
            if m:
                name, method, path = m.group(1).strip(), m.group(2).upper(), m.group(3)
            else:
                name, method, path = title, "", ""
            current = {"category": category, "endpoint_name": name,
                       "method": method, "path": path,
                       "source_url": "", "doc_version": "",
                       "parameters": [], "_raw": ln + "\n"}
            continue
        if current is None:
            pending.append(ln)
            continue
        current["_raw"] += ln + "\n"
        if not current["method"]:
            m = _METHOD_PATH.search(ln)
            if m:
                current["method"], current["path"] = m.group(1).upper(), m.group(2)
            else:
                m1 = _METHOD_LINE.search(ln)
                if m1:
                    current["method"] = m1.group(1).upper()
        if not current["path"] and current["method"]:
            m = _PATH_LINE.search(ln)
            if m:
                current["path"] = m.group(1)
        if not current["source_url"]:
            m = _SOURCE_URL.search(ln)
            if m:
                current["source_url"] = (m.group(1) or m.group(2) or "")
        if not current["doc_version"]:
            m = _DOC_VERSION.search(ln)
            if m:
                current["doc_version"] = m.group(1)
    flush()
    flush_pending()
    return eps, category_notes


def _parse_coverage(body: str) -> dict | None:
    pct = re.search(r"doc_coverage_pct[:\s]*([\d.]+)", body)
    if not pct:
        return None
    return {"doc_coverage_pct": float(pct.group(1)),
            "source": "migrated from md Document Coverage section"}


def convert(md_text: str) -> tuple[dict, dict]:
    """纯函数：md 文本 → (raw_knowledge.json dict, deployment_meta.json dict)。"""
    t = _TITLE.search(md_text)
    target = t.group(1).strip() if t else ""
    version = t.group(2).strip().lstrip("v") if t else ""

    meta, sources, coverage = {}, [], None
    endpoints, category_notes = [], {}
    deployment = {"sdk": {}, "docker_images": {}}
    keep_sections = []

    for title, body in _split_sections(md_text):
        tl = title.lower()
        if tl.startswith("document metadata"):
            meta = _parse_metadata(body)
        elif tl.startswith("document sources"):
            sources = _parse_sources(body)
        elif tl.startswith("sdk information"):
            deployment = _parse_deployment(body, "")
            # docker 章节稍后覆盖 docker_images；先记 sdk
        elif tl.startswith("docker images"):
            d2 = _parse_deployment("", body)
            deployment["docker_images"] = d2["docker_images"]
        elif tl.startswith(("api endpoints", "sql operations")):
            endpoints, category_notes = _parse_endpoints(body)
        elif tl.startswith("document coverage"):
            coverage = _parse_coverage(body)
        else:
            keep_sections.append({"title": title, "content": body.strip()})

    out = {
        "target": target,
        "version": version,
        "migrated_from_md": True,
        "migrated_at": datetime.now(timezone.utc).isoformat(),
        "document_metadata": meta,
        "document_sources": sources,
        "api_endpoints": endpoints,
        "category_notes": category_notes,
        "sections": keep_sections,
    }
    if coverage:
        out["openapi_coverage"] = coverage
    return out, deployment


def _self_check() -> int:
    failures = []

    def expect(c, m):
        if not c:
            failures.append(m)

    md = """# qdrant v1.18.0 API Knowledge

## Document Metadata
- doc_version: v1.18.x
- version_match: matched

## Document Sources
| # | URL | Doc Version | Fetched At | Version Match |
|---|-----|-------------|------------|---------------|
| 1 | https://a | v1.18 | t1 | matched |
| 2 | https://b | v1.18 | t2 | mismatched |

## SDK Information
- Package: qdrant-client
- Version: 1.13.0

## Docker Images
- Available tags: [1.18.0, 1.18.1]
- Recommended: 1.18.0

## Version-Gated Features (contract-critical)
- TurboQuant (v1.18+)

## API Endpoints

### Collections

#### Create a collection
- Method: PUT — Path: /collections/{collection_name}
- Source URL: https://a/create.md — Doc Version: v1.18.x
- Parameters:
  - vectors (object, optional): nested detail
- Constraints:
  - range: size min 1
- Expected Responses: 200; 400

#### Get collection details
- Method: GET
- Path: /collections/{collection_name}
- Source URL: https://a/get.md
- Doc Version: v1.18.x

   ### Points
   - Snapshot bulk list: GET /collections/{c}/snapshots; POST /collections/{c}/snapshots (create)

#### Overwrite payload — Method: PUT — Path: /collections/{c}/points/payload — 200/400
#### Recommend batch points — POST /points/recommend/batch — 200

## Data Types
- PointId: uint or uuid

## Document Coverage (OpenAPI cross-check)
- doc_coverage_pct: 95.2% (60/63)
"""
    out, dep = convert(md)
    expect(out["target"] == "qdrant" and out["version"] == "1.18.0", f"title parse: {out['target']}/{out['version']}")
    expect(out["document_metadata"]["doc_version"] == "v1.18.x", "metadata kv")
    expect(len(out["document_sources"]) == 2 and out["document_sources"][0]["url"] == "https://a", "sources table")
    expect(dep["sdk"]["package"] == "qdrant-client", "deployment sdk")
    expect(dep["docker_images"]["available_tags"] == "[1.18.0, 1.18.1]", "deployment docker")
    eps = out["api_endpoints"]
    expect(len(eps) == 4, f"4 endpoints, got {len(eps)}")
    e1 = eps[0]
    expect(e1["method"] == "PUT" and e1["path"] == "/collections/{collection_name}", f"single-line method-path: {e1['method']} {e1['path']}")
    expect(e1["category"] == "Collections", "category from ###")
    expect(e1["source_url"] == "https://a/create.md" and e1["doc_version"] == "v1.18.x", "source/version parse")
    expect("range: size min 1" in e1["raw_block"], "raw_block preserves constraints")
    e2 = eps[1]
    expect(e2["method"] == "GET" and e2["path"] == "/collections/{collection_name}", f"two-line method/path: {e2['method']} {e2['path']}")
    # 压缩格式（真实 md 变体）：标题内嵌 Method — Path / METHOD path + 缩进 ### category
    e3 = eps[2]
    expect(e3["method"] == "PUT" and e3["path"] == "/collections/{c}/points/payload", f"h4 embedded method-path: {e3['method']} {e3['path']}")
    expect(e3["category"] == "Points", f"indented category: {e3['category']}")
    e4 = eps[3]
    expect(e4["method"] == "POST" and e4["path"] == "/points/recommend/batch", f"h4 short form: {e4['method']} {e4['path']}")
    expect("Snapshot bulk list" in out["category_notes"].get("Points", ""), "loose lines → category_notes")
    titles = [s["title"] for s in out["sections"]]
    expect("Version-Gated Features (contract-critical)" in titles and "Data Types" in titles,
           f"keep_sections: {titles}")
    expect(out["openapi_coverage"]["doc_coverage_pct"] == 95.2, "coverage migrate")

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("migrate_raw_knowledge self-check OK")
    return 0


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "--self-check":
        sys.exit(_self_check())
    args = [a for a in sys.argv[1:]]
    dry = "--dry-run" in args
    args = [a for a in args if a != "--dry-run"]
    if not args:
        print("Usage: python scripts/migrate_raw_knowledge.py <dir> [--dry-run]", file=sys.stderr)
        sys.exit(2)
    d = Path(args[0])
    md_path = d / "raw_knowledge.md"
    if not md_path.exists():
        print(f"not found: {md_path}", file=sys.stderr)
        sys.exit(1)

    out, dep = convert(md_path.read_text(encoding="utf-8", errors="replace"))
    summary = {
        "target": out["target"], "version": out["version"],
        "endpoints": len(out["api_endpoints"]),
        "endpoints_with_method": sum(1 for e in out["api_endpoints"] if e["method"]),
        "sources": len(out["document_sources"]),
        "kept_sections": [s["title"] for s in out["sections"]],
    }
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    if dry:
        print("[migrate] dry-run — no file written")
        return
    (d / "raw_knowledge.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    (d / "deployment_meta.json").write_text(
        json.dumps(dep, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[migrate] written: {d/'raw_knowledge.json'} + {d/'deployment_meta.json'} (md kept)")


if __name__ == "__main__":
    main()
