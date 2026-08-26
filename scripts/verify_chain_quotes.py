"""verify_chain_quotes.py — 证据链引文逐字核对（R2 教训机制化：quote_mismatch 灰区前置拦截）。

R2 实测：3 条链因 builder 在 assertion_text_quoted 附加 '(description: ...)' 括注
落 NME 补证轮（3 builder + 1 auditor 成本）。本脚本在 auditor 前机械核对，
判定规则与 check_chain_grounding.py 完全一致（quote 必须是契约文件原文子串，
含 \\" 规范化双试）——auditor 判 quote_ok 的链本脚本必须放行。

用法：
    python scripts/verify_chain_quotes.py <SESSION_DIR> [--contract <path>]
退出码：0=全过或无契约；1=存在 mismatch（输出清单，主进程按工单打回对应 builder）
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("session_dir")
    ap.add_argument("--contract", default=None,
                    help="默认 SESSION_DIR 上一级 structured_contract.json")
    args = ap.parse_args()

    sd = args.session_dir
    contract = args.contract or os.path.join(
        os.path.dirname(os.path.abspath(sd)), "structured_contract.json")
    if not os.path.isfile(contract):
        print(f"[verify_chain_quotes] contract not found: {contract} — skip (no contract, not blocking)")
        return 0

    with open(contract, encoding="utf-8") as f:
        contract_text = f.read()
    norm = contract_text.replace('\\"', '"')  # 同 check_chain_grounding 的 JSON 转义规范化

    chains = sorted(glob.glob(os.path.join(sd, "evidence_chain", "*.json")))
    chains = [c for c in chains if not c.endswith(".done")]
    bad, unchecked = [], 0
    for cf in chains:
        try:
            with open(cf, encoding="utf-8") as f:
                chain = json.load(f)
        except (json.JSONDecodeError, OSError) as e:
            bad.append((os.path.basename(cf), f"json_error: {e}"))
            continue
        cg = (chain.get("steps") or {}).get("contract_grounding") or {}
        quoted = cg.get("assertion_text_quoted") or ""
        if not quoted:
            unchecked += 1  # 无引文（非约束型链）——留 auditor
            continue
        if not (quoted in contract_text or quoted in norm):
            bad.append((os.path.basename(cf),
                        f"quote not in contract (cid={cg.get('constraint_id', '?')})"))

    print(f"[verify_chain_quotes] chains={len(chains)} unchecked={unchecked} mismatch={len(bad)}")
    for name, why in bad:
        print(f"  MISMATCH {name}: {why}")
    if bad:
        print("[verify_chain_quotes] FAIL — 按 R2 工单模式打回对应 builder 重引原文子串")
        return 1
    print("[verify_chain_quotes] PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
