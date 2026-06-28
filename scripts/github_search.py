#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path
from typing import Optional, List, Dict, Any

import requests

CACHE_DIR = Path.home() / ".testvdb" / "github_cache"
CACHE_TTL = 86400

REPO_MAP = {
    "milvus": "milvus-io/milvus",
    "qdrant": "qdrant/qdrant",
    "weaviate": "weaviate/weaviate",
    "pgvector": "pgvector/pgvector",
    "meilisearch": "meilisearch/meilisearch",
    "chroma": "chroma-core/chroma",
}

GITHUB_API = "https://api.github.com/search/issues"


def _cache_path(query: str) -> Path:
    h = hashlib.sha256(query.encode()).hexdigest()
    return CACHE_DIR / f"{h}.json"


def _read_cache(query: str):
    p = _cache_path(query)
    if p.exists():
        data = json.loads(p.read_text(encoding="utf-8"))
        if time.time() - data.get("ts", 0) < CACHE_TTL:
            return data.get("items", [])
    return None


def _write_cache(query: str, items: list):
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    p = _cache_path(query)
    p.write_text(json.dumps({"ts": time.time(), "items": items}), encoding="utf-8")


def _build_query(target: str, defect_type: str, param_name: str) -> str:
    repo = REPO_MAP.get(target)
    if not repo:
        print(json.dumps({"error": f"unknown target: {target}"}), file=sys.stderr)
        sys.exit(1)
    return f"repo:{repo} {param_name} {defect_type} label:bug state:open"


def _score_title(title: str, param_name: str, defect_type: str) -> float:
    title_lower = title.lower()
    tokens = [param_name.lower(), defect_type.lower().replace("_", " ")]
    parts = param_name.lower().split(".")
    tokens.extend(p for p in parts if len(p) > 2)
    defect_parts = defect_type.lower().replace("_", " ").split()
    tokens.extend(p for p in defect_parts if len(p) > 2)
    matched = sum(1 for t in tokens if t in title_lower)
    total = len(tokens)
    if total == 0:
        return 0.0
    return round(matched / total, 4)


def _search_api(query: str, token: Optional[str]):
    headers = {"Accept": "application/vnd.github.v3+json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        resp = requests.get(GITHUB_API, params={"q": query, "per_page": 30}, headers=headers, timeout=15)
        remaining = int(resp.headers.get("X-RateLimit-Remaining", 999))
        if remaining < 5:
            cached = _read_cache(query)
            if cached is not None:
                return cached, True, remaining
        resp.raise_for_status()
        items = resp.json().get("items", [])
        _write_cache(query, items)
        return items, False, remaining
    except Exception as e:
        cached = _read_cache(query)
        if cached is not None:
            return cached, True, "cache-fallback"
        raise


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True, choices=REPO_MAP.keys())
    parser.add_argument("--defect-type", required=True)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--param-name", required=True)
    parser.add_argument("--github-token", default=None)
    args = parser.parse_args()

    query = _build_query(args.target, args.defect_type, args.param_name)
    token = args.github_token or os.environ.get("GITHUB_TOKEN")

    if token:
        try:
            items, from_cache, remaining = _search_api(query, token)
        except Exception as e:
            print(json.dumps({
                "warning": f"GitHub API error: {e}",
                "query": query,
                "duplicates": [],
            }, indent=2))
            return
    else:
        cached = _read_cache(query)
        if cached is not None:
            items, from_cache, remaining = cached, True, "cache-only"
        else:
            print(json.dumps({
                "warning": "No GITHUB_TOKEN and no cache available, skipping duplicate check",
                "query": query,
                "duplicates": [],
            }, indent=2))
            return

    results = []
    for item in items:
        score = _score_title(item.get("title", ""), args.param_name, args.defect_type)
        results.append({
            "html_url": item.get("html_url", ""),
            "title": item.get("title", ""),
            "state": item.get("state", ""),
            "similarity": score,
        })

    results.sort(key=lambda x: x["similarity"], reverse=True)

    output = {
        "query": query,
        "from_cache": from_cache,
        "rate_limit_remaining": remaining,
        "total_count": len(results),
        "duplicates": results,
    }
    print(json.dumps(output, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
