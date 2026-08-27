#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""_apply_script_retry.py — Stage 1 retry counter + feedback 写入 + 超限降级（确定性）。

配合 _classify_script_errors.py 用。LLM Orchestrator 维护 retry counter 不可靠（漏更新→
无限 retry / 删错文件），本脚本把"读 errors → 查 counter → 写 feedback / 删超限"全代码化。

借鉴 pipeline_state._handle_defect_review 的 retry 设计模式（counter + MAX_RETRY + 超限降级）。

输入：SESSION_DIR（含 script_errors.json 由 _classify_script_errors.py 产出）
副作用：
  - 更新 ${SESSION_DIR}/script_retry.json（per script_id counter，跨轮持久）
  - 对未超限：写 ${source}_scripts/${script_id}.retry_feedback.json（attack agent 读此修脚本）
  - 对超限：删 ${source}_scripts/${script_id}.py + .meta.json + 追加 script_retry_exhausted.json
输出（stdout JSON）：{regen: [{script_id, error_classes, retry_count}],
                     exhausted: [{script_id, error_classes, path}],
                     total_errors, max_retry}
Exit: 0 = ok（无论是否有 regen/exhausted；这是机制而非 gate）,
      2 = usage / 缺 script_errors.json
"""
from __future__ import annotations

import datetime
import hashlib
import json
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

MAX_SCRIPT_RETRY = 2  # 脚本颗粒度小，2 次足够（attack agent 一次调用可处理多个 feedback）


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: _apply_script_retry.py SESSION_DIR", file=sys.stderr)
        return 2
    sd = Path(sys.argv[1])
    if not sd.is_dir():
        print(f"FATAL: {sd} not a dir", file=sys.stderr)
        return 2

    errors_path = sd / "script_errors.json"
    if not errors_path.exists():
        # script_errors.json 不存在 = 上游 _classify_script_errors.py 还没跑
        # 不是错误（可能首次进入 Stage 1）—— 返回空结果
        print(json.dumps({
            "regen": [],
            "exhausted": [],
            "total_errors": 0,
            "max_retry": MAX_SCRIPT_RETRY,
            "note": "script_errors.json not found; run _classify_script_errors.py first",
        }, ensure_ascii=False, indent=2))
        return 0

    errors_data = json.loads(errors_path.read_text(encoding="utf-8"))
    errors: list[dict] = errors_data.get("errors", [])

    # 加载 retry counter（跨轮持久）
    retry_path = sd / "script_retry.json"
    retry_map: dict[str, int] = {}
    if retry_path.exists():
        try:
            retry_map = json.loads(retry_path.read_text(encoding="utf-8"))
            if not isinstance(retry_map, dict):
                retry_map = {}
        except (json.JSONDecodeError, OSError):
            retry_map = {}

    regen: list[dict] = []
    exhausted: list[dict] = []
    now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()

    # 幂等化（R4 挂账）：同 errors 集重复执行不双计——上次运行的 errors 指纹
    # 与本次一致则跳过计数（R4 实测 _apply 被重复跑导致 counter 2/2 触顶假风险）
    errors_sig = hashlib.sha256(json.dumps(
        sorted((e.get("script_id"), tuple(sorted(e.get("error_classes", []))))
               for e in errors), ensure_ascii=False).encode("utf-8")).hexdigest()
    sig_path = sd / "script_retry.sig"
    same_sig = sig_path.exists() and sig_path.read_text(encoding="utf-8").strip() == errors_sig
    if not same_sig:
        sig_path.write_text(errors_sig, encoding="utf-8")


    for e in errors:
        sid = e.get("script_id", "")
        spath_str = e.get("script_path", "")
        spath = Path(spath_str)
        classes = e.get("error_classes", [])
        hints = e.get("feedback_hints", {})
        # D3b severity（v3.4）：WARN 类不消耗 retry 预算、不进 regen——
        # 边车 preverify_warnings.json 由 _preverify_spec_shape 单独写，随脚本进执行队列
        sevs = e.get("severities") or {}
        if sevs:
            reject_classes = [c for c in classes if sevs.get(c, "REJECT") == "REJECT"]
            if not reject_classes:
                continue  # 全 WARN：跳过（不计数不打回）
            classes = reject_classes

        count = int(retry_map.get(sid, 0))
        if count < MAX_SCRIPT_RETRY:
            # 未超限：counter += 1，写 retry_feedback.json（幂等：同集重跑不计数）
            if not same_sig:
                count += 1
            retry_map[sid] = count
            fb = {
                "script_id": sid,
                "original_path": spath_str,
                "error_classes": classes,
                "feedback_hints": hints,
                "retry_count": count,
                "max_retry": MAX_SCRIPT_RETRY,
                "written_at": now_iso,
            }
            fb_path = spath.parent / f"{sid}.retry_feedback.json"
            try:
                fb_path.write_text(
                    json.dumps(fb, ensure_ascii=False, indent=2), encoding="utf-8")
            except OSError as ex:
                print(f"WARN: failed to write {fb_path}: {ex}", file=sys.stderr)
            regen.append({
                "script_id": sid,
                "error_classes": classes,
                "retry_count": count,
                "feedback_path": str(fb_path),
            })
        else:
            # 超限降级：删脚本 + .meta.json
            for p in [spath, spath.parent / f"{sid}.meta.json"]:
                if p.exists():
                    try:
                        p.unlink()
                    except OSError as ex:
                        print(f"WARN: failed to delete {p}: {ex}", file=sys.stderr)
            exhausted.append({
                "script_id": sid,
                "error_classes": classes,
                "path": spath_str,
                "retry_count": count,
            })

    # 写回 retry_map
    try:
        retry_path.write_text(
            json.dumps(retry_map, ensure_ascii=False, indent=2), encoding="utf-8")
    except OSError as ex:
        print(f"WARN: failed to write {retry_path}: {ex}", file=sys.stderr)

    # 追加 exhausted log（跨轮累积）
    if exhausted:
        ex_path = sd / "script_retry_exhausted.json"
        existing: list[dict] = []
        if ex_path.exists():
            try:
                existing = json.loads(ex_path.read_text(encoding="utf-8")).get("exhausted", [])
            except (json.JSONDecodeError, OSError):
                pass
        existing.extend(exhausted)
        try:
            ex_path.write_text(json.dumps({
                "exhausted": existing,
                "updated_at": now_iso,
            }, ensure_ascii=False, indent=2), encoding="utf-8")
        except OSError as ex:
            print(f"WARN: failed to write {ex_path}: {ex}", file=sys.stderr)

    result = {
        "regen": regen,
        "exhausted": exhausted,
        "total_errors": len(errors),
        "max_retry": MAX_SCRIPT_RETRY,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
