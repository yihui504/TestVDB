#!/usr/bin/env python3
"""Pre-fetch OpenAPI spec for a target/version into .sourcedeps/ (deterministic, no LLM).

Root-cause fix (pilot qdrant v1.18.2, 2026-08-20): knowledge-extractor Step 6b
的 OpenAPI cross-check 依赖 `.sourcedeps/...` 存在，"如不存在则跳过"是无条件
逃逸舱门——qdrant spec 从未被 fetch，extractor 只抓了 12 页却自报
doc_coverage_pct 100%。主进程在派 extractor **之前**跑本脚本把 spec 备好，
Step 6b 从"可选"变"有据可依"。

用法：
    py -3 scripts/fetch_openapi_spec.py {target} {version}

产出：
    .sourcedeps/{target}/{version}/openapi.json   # 合并后的单文件 spec（paths+components）

per-target 规则：
    qdrant    GitHub tag 单文件（v3.4 pilot 根因修复 2026-08-25）：
              raw …/{tag}/docs/redoc/{major.minor}.x/openapi.json
              （tag 树内快照 = 版本正确。**旧版抓 api.qdrant.tech/latest——无视版本参数，
              三份缓存全是 latest 快照，formalizer 规则 2.8 字节级核对拦获，已废弃**；
              404 = tag 不存在或结构变更，fail fast 不降级）
    weaviate  https://raw.githubusercontent.com/weaviate/weaviate/{tag}/openapi-specs/schema.json
    milvus    GitHub docs 仓库 openapi（若无公开 spec 则报 not_supported，退出 3）
    其余      not_supported（退出 3，主进程继续跑，不阻塞）
"""
import base64
import json
import os
import re
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

UA = {"User-Agent": "TestVDB-fetch-openapi/1.0"}
TIMEOUT = 30


def _get(url: str, binary: bool = False):
    headers = dict(UA)
    # GitHub contents API 匿名限流 60/h —— 有 token 就带上（raw 不需要但无害）
    if url.startswith("https://api.github.com/"):
        token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
        if token:
            headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        data = r.read()
    return data if binary else data.decode("utf-8", errors="replace")


def _fetch_qdrant(version: str) -> dict:
    """qdrant 按 tag：docs/redoc/{major.minor}.x/openapi.json 单文件（v1.18.0 实测结构）。

    tag 树内的 spec 即该版本发布时刻的 spec——版本正确性由来源保证（非 latest 快照）。
    ⛔ 禁止回退 api.qdrant.tech（latest 站点，v3.4 pilot 拦获的缺陷源）。
    """
    tag = version if version.startswith("v") else f"v{version}"
    core = tag.lstrip("v")                     # 1.18.0
    # 注意：redoc 目录名带 v 前缀（v1.18.x，非 1.18.x）——v3.4 实测踩坑
    minor_series = "v" + ".".join(core.split(".")[:2]) + ".x"   # v1.18.x
    api = (f"https://api.github.com/repos/qdrant/qdrant/contents/"
           f"docs/redoc/{minor_series}/openapi.json?ref={tag}")
    # 首选 contents API base64（raw.githubusercontent 在本机网络不稳，实测 HEAD 无响应；
    # contents API 走 api.github.com 可达且有 token 提额）。文件 >1MB 时 API 不回
    # content → 回退 download_url（raw）。
    item = json.loads(_get(api))
    if item.get("encoding") == "base64" and item.get("content"):
        spec = json.loads(base64.b64decode(item["content"]).decode("utf-8"))
    elif item.get("download_url"):
        spec = json.loads(_get(item["download_url"]))
    else:
        raise RuntimeError(f"qdrant {tag}: spec 取回失败（无 content 无 download_url）")
    if not (spec.get("paths") or {}):
        raise RuntimeError(f"qdrant {tag}: spec paths 为空（{api}）")
    return spec


def _fetch_weaviate(version: str) -> dict:
    tag = version if version.startswith("v") else f"v{version}"
    raw = _get(f"https://raw.githubusercontent.com/weaviate/weaviate/{tag}/openapi-specs/schema.json")
    return json.loads(raw)


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: fetch_openapi_spec.py {target} {version}", file=sys.stderr)
        return 2
    target, version = sys.argv[1], sys.argv[2]
    out_dir = Path(f".sourcedeps/{target}/{version}")
    out_path = out_dir / "openapi.json"

    if target == "qdrant":
        spec = _fetch_qdrant(version)
        source_tag = version if version.startswith("v") else f"v{version}"
    elif target == "weaviate":
        spec = _fetch_weaviate(version)
        source_tag = version if version.startswith("v") else f"v{version}"
    else:
        print(f"[fetch-openapi] {target}: no deterministic spec rule, skip")
        return 3

    out_dir.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(spec, ensure_ascii=False), encoding="utf-8")
    # sidecar：版本化来源标记（formalizer 规则 2.8 版本核对的对照物）
    (out_dir / "openapi.meta.json").write_text(json.dumps({
        "source": f"github:{'qdrant/qdrant' if target == 'qdrant' else 'weaviate/weaviate'}@{source_tag}",
        "fetched_at": datetime.now(timezone.utc).isoformat(),
        "versioned": True,
    }, ensure_ascii=False, indent=2), encoding="utf-8")
    n_paths = len(spec.get("paths") or {})
    print(f"[fetch-openapi] {target}/{version}@{source_tag}: {n_paths} paths -> {out_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"[fetch-openapi] FAIL: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        raise SystemExit(1)
