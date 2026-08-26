"""oracle_stats.py — Oracle/Attack 行结构化统计（v3.4 D3a 下游消费 + F 节 RQ1 数据层）。

数据源（全部机械解析，0 LLM）：
  debate_logs/*.py            docstring 的 Attack:/Oracle:/Constraint:/constraint_ids 行
  debate_logs/*.meta.json     endpoint/param/expected_defect_type/strategy
  exit_code_*.txt             执行结果
  candidates.jsonl            候选与触发 log
  debate_logs/chain_verdicts*.json  终判（多文件按文件名序合并，后写覆盖）

输出：SESSION_DIR/oracle_stats.json（records + pivots）
  pivots: verdict 分布 × 视角 / constraint_id / 策略（meta.strategy）/ oracle 覆盖率

用法：
    python scripts/oracle_stats.py <SESSION_DIR>
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import re
import sys
from collections import Counter, defaultdict

DOC_KEYS = ("Attack:", "Oracle:", "Constraint:")


def _parse_docstring(path: str) -> dict:
    """提取 docstring 头部的 Attack:/Oracle:/Constraint: 行 + constraint_ids 数组行。"""
    out: dict = {"attack": "", "oracle": "", "constraint_line": "", "constraint_ids": []}
    try:
        text = open(path, encoding="utf-8", errors="replace").read(6000)
    except OSError:
        return out
    in_doc = False
    for line in text.splitlines():
        t = line.strip()
        if not in_doc:
            if re.match(r'^("""|\'\'\')', t):
                in_doc = True
            continue
        if re.match(r'^("""|\'\'\')', t):
            break
        if t.startswith("Attack:") and not out["attack"]:
            out["attack"] = t[len("Attack:"):].strip()
        elif t.startswith("Oracle:") and not out["oracle"]:
            out["oracle"] = t[len("Oracle:"):].strip()
        elif t.startswith("Constraint:") and not out["constraint_line"]:
            out["constraint_line"] = t[len("Constraint:"):].strip()
        m = re.match(r'^constraint_ids:\s*(\[.*\])\s*$', t)
        if m:
            try:
                out["constraint_ids"] = json.loads(m.group(1))
            except json.JSONDecodeError:
                pass
    return out


def _load_verdicts(sd: str) -> dict:
    final: dict = {}
    for vf in sorted(glob.glob(os.path.join(sd, "debate_logs", "chain_verdicts*.json"))):
        if vf.endswith(".done"):
            continue
        try:
            d = json.load(open(vf, encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            continue
        items = d if isinstance(d, list) else d.get("verdicts", d.get("items", []))
        for v in items:
            if isinstance(v, dict) and v.get("defect_id"):
                final[v["defect_id"]] = v.get("verdict", "")
    return final


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("session_dir")
    args = ap.parse_args()
    sd = args.session_dir

    scripts = sorted(glob.glob(os.path.join(sd, "debate_logs", "*.py")))
    verdicts = _load_verdicts(sd)
    records = []
    for sp in scripts:
        sid = os.path.basename(sp)[:-3]
        rec = {"script_id": sid, "perspective": sid.split("_")[0]}
        rec.update(_parse_docstring(sp))
        mp = sp[:-3] + ".meta.json"
        if os.path.isfile(mp):
            try:
                meta = json.load(open(mp, encoding="utf-8"))
                rec["endpoint"] = meta.get("endpoint", "")
                rec["param"] = meta.get("param")
                rec["strategy"] = meta.get("strategy", "")
                rec["expected_defect_type"] = meta.get("expected_defect_type", "")
            except (json.JSONDecodeError, OSError):
                pass
        ef = os.path.join(sd, f"exit_code_{sid}.txt")
        if os.path.isfile(ef):
            rec["exit_code"] = open(ef, encoding="utf-8", errors="replace").read().strip()
        rec["verdict"] = verdicts.get(sid, "")
        rec["has_oracle"] = bool(rec["oracle"])
        records.append(rec)

    piv = {
        "scripts": len(records),
        "oracle_coverage": f"{sum(r['has_oracle'] for r in records)}/{len(records)}",
        "by_perspective": dict(Counter(r["perspective"] for r in records)),
        "verdict_total": dict(Counter(r["verdict"] for r in records if r["verdict"])),
        "defect_by_constraint": dict(Counter(
            c.split("::")[-1]  # unit_ref 形态（constraints::xxx）归一为裸 constraint_id
            for r in records if r["verdict"] == "DEFECT" for c in r["constraint_ids"])),
        "defect_by_strategy": dict(Counter(
            r.get("strategy", "?") for r in records if r["verdict"] == "DEFECT")),
        "candidates_by_perspective": dict(Counter(
            r["perspective"] for r in records if r["verdict"])),
    }
    out = {"session_dir": sd, "records": records, "pivots": piv}
    of = os.path.join(sd, "oracle_stats.json")
    with open(of, "w", encoding="utf-8", newline="\n") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
    print(json.dumps(piv, ensure_ascii=False, indent=1))
    print(f"[oracle_stats] written: {of}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
