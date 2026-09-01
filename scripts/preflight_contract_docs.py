#!/usr/bin/env python3
"""preflight_contract_docs.py — 契约文档资产预检（规则 P1.0，2026-09-02）。

契约加载时（mine Step 6.2 / contract Step 5b，门控 PASS 后）对契约内全部 source_url
去重批量预检：HTTP 可达性 + URL 版本 slug 与 target 比对。结果写 sidecar
doc_preflight.json（**契约文件本体零改动**）。evidence-builder A 层前两层消费
sidecar（有记录采信；dead/unreachable 自行复核一次；无记录回退现行 WebFetch）。

判定：
  http:    200/301/302 → reachable；404/410 → dead；其余/超时/连接错误 →
           unreachable（重试 1 次；网络失败与文档死亡分开，抖动不定罪）
  version: URL 版本 slug（v-1-18-x / v2.4.x 等）与 target major.minor 比对 →
           matched / mismatched / no_version_in_url（legacy 概念文档无版本路由，
           记 PARTIAL 不判 FAIL）

退出码：dead 或 mismatched 存在 → 1（**主进程记录不中断**——环境事实，builder
如实取证）；仅 network_error → 0 + summary 计数；TESTVDB_OFFLINE=1 → skipped，0。

Usage: python scripts/preflight_contract_docs.py <contract_path> [--timeout 10] [--concurrency 8]
"""
from __future__ import annotations

import json
import os
import re
import sys
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

SIDECAR_NAME = "doc_preflight.json"
_VERSION_RE = re.compile(r"/v[-.]?(\d+)[.-](\d+)")
_TARGET_RE = re.compile(r"^v?(\d+)\.(\d+)")
_HEADERS = {"User-Agent": "TestVDB-preflight/1.0"}


def extract_version_token(url: str):
    """从 URL 抽版本 slug 的 major.minor（v-1-18-x → (1,18)；v2.4.x → (2,4)）；无 → None。"""
    m = _VERSION_RE.search(url or "")
    return (int(m.group(1)), int(m.group(2))) if m else None


def classify_http(status: int) -> str:
    if status in (200, 301, 302):
        return "reachable"
    if status in (404, 410):
        return "dead"
    return "unreachable"


def classify_version(token, target_version: str) -> str:
    if token is None:
        return "no_version_in_url"
    m = _TARGET_RE.match(target_version or "")
    if not m:
        return "no_version_in_url"
    return "matched" if token == (int(m.group(1)), int(m.group(2))) else "mismatched"


def collect_urls(contract: dict) -> list:
    """递归收集契约内全部 source_url（端点注册表+约束+断言，去重保序）。"""
    seen, out = set(), []

    def walk(node):
        if isinstance(node, dict):
            u = node.get("source_url")
            if isinstance(u, str) and u.startswith("http") and u not in seen:
                seen.add(u)
                out.append(u)
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(contract)
    return out


def fetch_status(url: str, timeout: int):
    """GET 取状态码；异常返回 None（网络薄壳——单测不触）。"""
    import requests
    try:
        r = requests.get(url, timeout=timeout, headers=_HEADERS, allow_redirects=True)
        return r.status_code
    except Exception:
        return None


def preflight_url(url: str, target_version: str, timeout: int) -> dict:
    status = fetch_status(url, timeout)
    if status is None:  # 网络/超时 → 重试 1 次
        status = fetch_status(url, timeout)
    http = classify_http(status) if status is not None else "unreachable"
    return {"http": http,
            "version": classify_version(extract_version_token(url), target_version),
            "note": ""}


def main() -> int:
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("contract")
    ap.add_argument("--timeout", type=int, default=10)
    ap.add_argument("--concurrency", type=int, default=8)
    a = ap.parse_args()

    if os.environ.get("TESTVDB_OFFLINE") == "1":
        print("preflight: TESTVDB_OFFLINE=1 → skipped（builder 走回退）")
        return 0

    cpath = Path(a.contract)
    contract = json.loads(cpath.read_text(encoding="utf-8"))
    urls = collect_urls(contract)
    target_version = str(contract.get("version", ""))

    with ThreadPoolExecutor(max_workers=max(1, a.concurrency)) as ex:
        results = dict(ex.map(lambda u: (u, preflight_url(u, target_version, a.timeout)), urls))

    summary = {
        "urls": len(results),
        "reachable": sum(1 for r in results.values() if r["http"] == "reachable"),
        "dead": sum(1 for r in results.values() if r["http"] == "dead"),
        "unreachable": sum(1 for r in results.values() if r["http"] == "unreachable"),
        "version_matched": sum(1 for r in results.values() if r["version"] == "matched"),
        "mismatched": sum(1 for r in results.values() if r["version"] == "mismatched"),
        "no_version_in_url": sum(1 for r in results.values() if r["version"] == "no_version_in_url"),
    }
    sidecar = {"preflight_version": "P1.0", "ran_at": datetime.now().isoformat(timespec="seconds"),
               "target_version": target_version, "results": results, "summary": summary}
    out = cpath.parent / SIDECAR_NAME
    out.write_text(json.dumps(sidecar, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"preflight: {len(results)} urls → {out}")
    print(f"  http: reachable={summary['reachable']} dead={summary['dead']} "
          f"unreachable={summary['unreachable']}")
    print(f"  version: matched={summary['version_matched']} mismatched={summary['mismatched']} "
          f"no_version_in_url={summary['no_version_in_url']}")
    fatal = summary["dead"] + summary["mismatched"]
    if fatal:
        print(f"RESULT: WARN（{fatal} dead/mismatched — 主进程记录不中断，builder 如实取证）")
        return 1
    print("RESULT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
