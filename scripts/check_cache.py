#!/usr/bin/env python3
"""check_cache — 批次 D 决策 4 的 D 判断（全条件缓存复用检测）。

判断 intel/contract 缓存是否可复用：存在 + TTL 新鲜 + 有效 + target/version 匹配。
任一不满足 → 对应状态（MISSING/STALE/INVALID/MISMATCH）。

Usage（被 mine.md 引用）:
  python scripts/check_cache.py intel <intel_dir> <target> [--ttl HOURS]
  python scripts/check_cache.py contract <version_dir> <target> <version> [--ttl HOURS]
Exit: 0=USABLE, 1=STALE, 2=INVALID, 3=MISMATCH, 4=MISSING
"""
from __future__ import annotations

import json
import os
import sys
import time
from dataclasses import dataclass
from enum import Enum

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


class CacheStatus(Enum):
    USABLE = "usable"       # 全条件满足，可复用
    MISSING = "missing"     # 缓存文件不存在
    STALE = "expired"       # 存在但过期（超 TTL）
    INVALID = "invalid"     # 存在且未过期，但内容无效（缺必需字段）
    MISMATCH = "mismatch"   # target/version 不匹配


# CLI exit code 映射（main 用）
_EXIT_CODE = {
    CacheStatus.USABLE: 0,
    CacheStatus.STALE: 1,
    CacheStatus.INVALID: 2,
    CacheStatus.MISMATCH: 3,
    CacheStatus.MISSING: 4,
}


@dataclass(frozen=True)
class CacheResult:
    status: CacheStatus
    reason: str
    path: str


def _file_age_hours(path: str) -> float:
    return (time.time() - os.path.getmtime(path)) / 3600


def check_intel_cache(intel_dir: str, target: str, ttl_hours: int = 720) -> CacheResult:
    """D 判断情报缓存。intel_dir = intelligence/<target>/。"""
    tm_path = os.path.join(intel_dir, "threat_model.json")
    if not os.path.exists(tm_path):
        return CacheResult(CacheStatus.MISSING,
                           f"threat_model.json 不存在: {intel_dir}", intel_dir)
    if _file_age_hours(tm_path) > ttl_hours:
        return CacheResult(CacheStatus.STALE, f"过期（>{ttl_hours}h）", tm_path)
    try:
        with open(tm_path, encoding="utf-8") as f:
            tm = json.load(f)
    except (json.JSONDecodeError, OSError):
        return CacheResult(CacheStatus.INVALID, "JSON 解析失败", tm_path)
    # 有效性：含 cognitive_blindspots + attack_surface
    if not tm.get("cognitive_blindspots") or not tm.get("attack_surface"):
        return CacheResult(CacheStatus.INVALID,
                           "缺 cognitive_blindspots/attack_surface", tm_path)
    return CacheResult(CacheStatus.USABLE, "缓存有效", tm_path)


def check_contract_cache(version_dir: str, target: str, version: str,
                         ttl_hours: int = 168) -> CacheResult:
    """D 判断契约缓存。version_dir = results/<target>/<version>/。"""
    c_path = os.path.join(version_dir, "structured_contract.json")
    if not os.path.exists(c_path):
        return CacheResult(CacheStatus.MISSING,
                           f"structured_contract.json 不存在: {version_dir}", version_dir)
    if _file_age_hours(c_path) > ttl_hours:
        return CacheResult(CacheStatus.STALE, f"过期（>{ttl_hours}h）", c_path)
    try:
        with open(c_path, encoding="utf-8") as f:
            c = json.load(f)
    except (json.JSONDecodeError, OSError):
        return CacheResult(CacheStatus.INVALID, "JSON 解析失败", c_path)
    # 有效性先（spec 决策 4 顺序：存在→TTL→有效性→target/version 匹配）
    if not c.get("api_endpoints") or not c.get("data_types"):
        return CacheResult(CacheStatus.INVALID,
                           "缺 api_endpoints/data_types", c_path)
    # target/version 匹配（有效性之后）
    if str(c.get("target", "")).lower() != target.lower():
        return CacheResult(CacheStatus.MISMATCH,
                           f"target 不匹配（缓存={c.get('target')}, 请求={target}）", c_path)
    if str(c.get("version", "")).lower() != version.lower():
        return CacheResult(CacheStatus.MISMATCH,
                           f"version 不匹配（缓存={c.get('version')}, 请求={version}）", c_path)
    return CacheResult(CacheStatus.USABLE, "缓存有效", c_path)


def _parse_ttl(argv, default_ttl):
    """从 argv 解析 --ttl HOURS，找不到则返回 default_ttl。"""
    if "--ttl" in argv:
        i = argv.index("--ttl")
        if i + 1 < len(argv):
            try:
                return int(argv[i + 1])
            except ValueError:
                pass
    return default_ttl


def main():
    if len(sys.argv) < 4:
        print("Usage: check_cache.py {intel|contract} <dir> <target> [version] [--ttl HOURS]",
              file=sys.stderr)
        return 2
    kind = sys.argv[1]
    dir_ = sys.argv[2]
    target = sys.argv[3]

    if kind == "intel":
        ttl = _parse_ttl(sys.argv, 720)
        r = check_intel_cache(dir_, target, ttl)
    elif kind == "contract":
        if len(sys.argv) < 5:
            print("Usage: check_cache.py contract <dir> <target> <version> [--ttl HOURS]",
                  file=sys.stderr)
            return 2
        version = sys.argv[4]
        ttl = _parse_ttl(sys.argv, 168)
        r = check_contract_cache(dir_, target, version, ttl)
    else:
        print(f"未知 kind: {kind}", file=sys.stderr)
        return 2

    print(f"{kind} cache: {r.status.value} — {r.reason} ({r.path})")
    return _EXIT_CODE[r.status]


if __name__ == "__main__":
    sys.exit(main())
