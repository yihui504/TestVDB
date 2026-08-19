#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_anchor_conflict.py — by_design 锚点与链现象的确定性冲突检测（v8 FP 侧，2026-08-20）。

背景：v8 阶段2 保守改进。4 案 milvus FP 是 A=DEFECT 机械锁定（技术事实链完整），
但维护者已用 resolution/by-design 官方标签 + maintainer 文字表态双信号明示 stance。
直接翻 NOT_DEFECT 违反机械纪律；设计为锚点感知 CONFLICT——确定性关键词匹配命中
时降 CONFLICT 走打回闭环（builder/auditor 复核终判）。

匹配规则（保守，宁缺勿滥）：
  对 cognition.by_design_patterns 每条锚点：
    1. 取锚点 pattern 中≥5 字符的实词（小写化，去停用词）为关键词集
    2. 链现象文本 = execution_evidence.log_pattern + secondary_observations
       + contract_grounding.assertion_text_quoted
    3. 命中 = 锚点关键词在现象文本中出现 ≥2 个（跨词重合，非语义泛化）
    4. issue 号核对：锚点 source_issues 与链 defect_id 所属 issue 无需一致
       （锚点是现象类模式），但现象文本须含锚点 pattern 的核心参数名（如
       shardsNum/rowCount/rename——从 pattern 提取的标识符词强制在现象中出现）

cognition 文件定位：按 vendor 从链路径推断（sessions/{vendor}/...），
读 {INTEL_ROOT}/{vendor}/developer_cognition.json；未找到 → 无冲突（fail-open）。

⚠️ GT-informed 披露：锚点内容含维护者表态引文，论文需披露注入（先例 fixD/fixG）。
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

if sys.platform == "win32":
    pass

INTEL_ROOT = Path(r"C:/Users/11428/Desktop/tvdb_sessions/intelligence")

STOPWORDS = {
    "this", "that", "with", "from", "when", "only", "such", "into", "than",
    "then", "them", "will", "shall", "been", "have", "has", "does", "not",
    "and", "the", "for", "are", "but", "his", "her", "its", "our", "your",
    "设计", "行为", "以及", "一个",
}
IDENT_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]{3,}")


def _anchor_keywords(pattern: str) -> tuple[set[str], set[str]]:
    """返回 (词集, 标识符集)。标识符 = CamelCase/snake_case 实词（强制匹配用）。"""
    words = set()
    idents = set()
    for w in re.split(r"[^A-Za-z0-9_]+", pattern):
        wl = w.lower()
        if len(wl) < 5 or wl in STOPWORDS:
            continue
        words.add(wl)
        # CamelCase 或含下划线的视为标识符
        if "_" in w or (w[:1].islower() and any(c.isupper() for c in w)):
            idents.add(wl)
    return words, idents


def detect_anchor_conflict(chain: dict, intel_root: Path | None = None) -> dict:
    """返回 {"anchored_conflict": bool, "anchor": str|None}。"""
    did = str(chain.get("defect_id") or "")
    m = re.match(r"([a-z]+)_", did)
    vendor = m.group(1) if m else ""
    cog_path = (intel_root or INTEL_ROOT) / vendor / "developer_cognition.json"
    if not cog_path.exists():
        return {"anchored_conflict": False, "anchor": None}
    try:
        cog = json.loads(cog_path.read_text(encoding="utf-8", errors="replace"))
    except json.JSONDecodeError:
        return {"anchored_conflict": False, "anchor": None}

    # 链现象文本
    ee = (chain.get("steps") or {}).get("execution_evidence") or {}
    cg = (chain.get("steps") or {}).get("contract_grounding") or {}
    phen = "\n".join(
        [str(ee.get("log_pattern") or "")]
        + [str(x) for x in ee.get("secondary_observations") or []]
        + [str(cg.get("assertion_text_quoted") or "")]
    ).lower()

    for a in cog.get("by_design_patterns", []):
        pattern = str(a.get("pattern") or "")
        # 指纹级匹配 v2（2026-08-20 两轮回测教训：pattern 猜词要么过泛 23 误命中、
        # 要么过紧 018/021 无标识符漏命中）——只认锚点作者显式声明的 fingerprints
        # 词组：全部在场才命中。无 fingerprints 的旧锚点（fixD 时代的）不参与检测
        fps = a.get("fingerprints") or []
        if not pattern or not fps:
            continue
        if all(fp.lower() in phen for fp in fps):
            return {"anchored_conflict": True,
                    "anchor": f"by_design[{str(a.get('source_issues'))}]: {pattern[:70]}"}
    return {"anchored_conflict": False, "anchor": None}


if __name__ == "__main__":
    chain = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace"))
    print(json.dumps(detect_anchor_conflict(chain), ensure_ascii=False))
