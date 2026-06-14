# 批次 D · 命令解耦（Command Decoupling）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 TestVDB 三阶段（情报/契约/挖掘）从单一 `/testvdb:mine` 拆成三个可独立触发、智能协作的子命令（`/testvdb:intel`、`/testvdb:contract`、`/testvdb:mine`），mine 智能 D 判断（缓存+TTL+有效性+target/version）+ 参数控制 + C 边界。

**Architecture:** 提取 `mine.md` 的情报（Step 3.6）+ 契约（Step 4-6）阶段到独立 `commands/intel.md` + `contract.md`；`mine.md` 重构为"智能消费 intel/contract 缓存 + 挖掘编排"；新增 `scripts/check_cache.py` 做 D 判断（全条件复用检测）。

**Tech Stack:** Python（check_cache.py TDD）、Markdown（commands SOP）、JSON（plugin.json 注册）。本环境 CC 2.1.177 proxy 无 subagent，实现需在 CC 2.1.165 会话或主进程 inline（commands 是 SOP 文档，inline 可写；端到端验证需 2.1.165）。

**Spec:** `docs/superpowers/specs/2026-06-14-batch-d-command-decoupling-design.md`

---

## File Structure

| 文件 | 操作 | 责任 |
|------|------|------|
| `scripts/check_cache.py` | 新建 | D 判断：intel/contract 缓存存在+TTL新鲜+有效性+target/version 匹配 → 是否可复用 |
| `tests/test_check_cache.py` | 新建 | check_cache TDD（pytest） |
| `commands/contract.md` | 新建 | 契约生成 SOP（提取自 mine.md Step 4-6 + --force 参数） |
| `commands/intel.md` | 新建 | 情报采集 SOP（提取自 mine.md Step 3.6 + --max-issues/--max-commits 参数） |
| `.claude-plugin/plugin.json` | 修改 | commands 数组加 intel.md/contract.md |
| `commands/mine.md` | 修改 | 重构：情报/契约阶段提取 → 引用 intel/contract 命令逻辑 + 智能 D 判断 + --intel/--contract 参数 + C 边界 |
| `agents/orchestrator.md` | 修改 | 瘦化（mine.md 重构后编排引用更新，顺带解 834 行） |

**测试约定**：`check_cache.py` 用 pytest TDD；commands（markdown SOP）用 grep 验证 + 端到端（2.1.165 跑命令）。

---

## Task 1: check_cache.py — D 判断核心（TDD）

**Files:**
- Create: `scripts/check_cache.py`
- Test: `tests/test_check_cache.py`

check_cache 实现 spec 决策 4（D 判断全条件）：缓存存在 + TTL 新鲜 + 有效性 + target/version 匹配。

- [ ] **Step 1: 写失败测试**

Create `tests/test_check_cache.py`:

```python
"""check_cache D 判断测试（批次 D 决策 4：缓存+TTL+有效性+target/version）。"""
import json
import time
from pathlib import Path

from check_cache import check_intel_cache, check_contract_cache, CacheStatus


def _make_intel(root, target="weaviate", age_hours=10, valid=True, ttl_hours=720):
    """造 intelligence/<target>/threat_model.json。"""
    d = root / "intelligence" / target
    d.mkdir(parents=True)
    tm = {"cognitive_blindspots": {"blindspots": ["bs1"]}, "attack_surface": {"high_priority_areas": []}}
    if not valid:
        tm = {}  # 无效（缺字段）
    p = d / "threat_model.json"
    p.write_text(json.dumps(tm), encoding="utf-8")
    import os, datetime
    ts = time.time() - age_hours * 3600
    os.utime(str(p), (ts, ts))
    return p


def _make_contract(root, target="weaviate", version="1.38.0", age_hours=10, valid=True, ttl_hours=168):
    """造 results/<target>/<version>/structured_contract.json。"""
    d = root / "results" / target / version
    d.mkdir(parents=True)
    c = {"target": target, "version": version, "api_endpoints": [
        {"path": "/v1/objects", "method": "POST", "category": "data", "source_url": "u"}],
        "data_types": [{"name": "vector"}]}
    if not valid:
        c = {"target": target}  # 无效（缺字段）
    p = d / "structured_contract.json"
    p.write_text(json.dumps(c), encoding="utf-8")
    import os
    ts = time.time() - age_hours * 3600
    os.utime(str(p), (ts, ts))
    return p


def test_intel_cache_fresh_and_valid(tmp_path):
    """情报缓存：存在+未过期+有效 → USABLE。"""
    _make_intel(tmp_path, age_hours=10, valid=True)
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate", ttl_hours=720)
    assert result.status == CacheStatus.USABLE


def test_intel_cache_missing(tmp_path):
    """情报缓存不存在 → MISSING。"""
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate")
    assert result.status == CacheStatus.MISSING


def test_intel_cache_expired(tmp_path):
    """情报缓存过期（age > TTL）→ STALE。"""
    _make_intel(tmp_path, age_hours=1000, valid=True)  # 1000h > 720h TTL
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate", ttl_hours=720)
    assert result.status == CacheStatus.STALE


def test_intel_cache_invalid(tmp_path):
    """情报缓存无效（缺 threat_model 字段）→ INVALID。"""
    _make_intel(tmp_path, age_hours=10, valid=False)
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate")
    assert result.status == CacheStatus.INVALID


def test_contract_cache_fresh_and_valid(tmp_path):
    """契约缓存：存在+未过期+有效+target/version 匹配 → USABLE。"""
    _make_contract(tmp_path, target="weaviate", version="1.38.0", age_hours=10, valid=True)
    result = check_contract_cache(
        str(tmp_path / "results" / "weaviate" / "1.38.0"), "weaviate", "1.38.0", ttl_hours=168)
    assert result.status == CacheStatus.USABLE


def test_contract_cache_target_mismatch(tmp_path):
    """契约缓存 target/version 不匹配 → MISMATCH。"""
    _make_contract(tmp_path, target="qdrant", version="1.18.2")  # 缓存是 qdrant
    result = check_contract_cache(
        str(tmp_path / "results" / "qdrant" / "1.18.2"), "weaviate", "1.38.0")  # 请求 weaviate
    assert result.status == CacheStatus.MISMATCH


def test_contract_cache_invalid(tmp_path):
    """契约缓存无效（缺 api_endpoints）→ INVALID。"""
    _make_contract(tmp_path, valid=False)
    result = check_contract_cache(
        str(tmp_path / "results" / "weaviate" / "1.38.0"), "weaviate", "1.38.0")
    assert result.status == CacheStatus.INVALID
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd C:\Users\11428\Desktop\mftui\TestVDB && PYTHONUTF8=1 python -m pytest tests/test_check_cache.py -v`
Expected: FAIL — `check_cache` 模块不存在。

- [ ] **Step 3: 实现 check_cache.py**

Create `scripts/check_cache.py`:

```python
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
        return CacheResult(CacheStatus.MISSING, f"threat_model.json 不存在: {intel_dir}", intel_dir)
    if _file_age_hours(tm_path) > ttl_hours:
        return CacheResult(CacheStatus.STALE, f"过期（>{ttl_hours}h）", tm_path)
    try:
        with open(tm_path, encoding="utf-8") as f:
            tm = json.load(f)
    except (json.JSONDecodeError, OSError):
        return CacheResult(CacheStatus.INVALID, "JSON 解析失败", tm_path)
    # 有效性：含 cognitive_blindspots + attack_surface
    if not tm.get("cognitive_blindspots") or not tm.get("attack_surface"):
        return CacheResult(CacheStatus.INVALID, "缺 cognitive_blindspots/attack_surface", tm_path)
    return CacheResult(CacheStatus.USABLE, "缓存有效", tm_path)


def check_contract_cache(version_dir: str, target: str, version: str, ttl_hours: int = 168) -> CacheResult:
    """D 判断契约缓存。version_dir = results/<target>/<version>/。"""
    c_path = os.path.join(version_dir, "structured_contract.json")
    if not os.path.exists(c_path):
        return CacheResult(CacheStatus.MISSING, f"structured_contract.json 不存在: {version_dir}", version_dir)
    if _file_age_hours(c_path) > ttl_hours:
        return CacheResult(CacheStatus.STALE, f"过期（>{ttl_hours}h）", c_path)
    try:
        with open(c_path, encoding="utf-8") as f:
            c = json.load(f)
    except (json.JSONDecodeError, OSError):
        return CacheResult(CacheStatus.INVALID, "JSON 解析失败", c_path)
    # target/version 匹配
    if str(c.get("target", "")).lower() != target.lower():
        return CacheResult(CacheStatus.MISMATCH, f"target 不匹配（缓存={c.get('target')}, 请求={target}）", c_path)
    if str(c.get("version", "")).lower() != version.lower():
        return CacheResult(CacheStatus.MISMATCH, f"version 不匹配（缓存={c.get('version')}, 请求={version}）", c_path)
    # 有效性：含 api_endpoints + data_types
    if not c.get("api_endpoints") or not c.get("data_types"):
        return CacheResult(CacheStatus.INVALID, "缺 api_endpoints/data_types", c_path)
    return CacheResult(CacheStatus.USABLE, "缓存有效", c_path)


def main():
    if len(sys.argv) < 4:
        print("Usage: check_cache.py {intel|contract} <dir> <target> [version] [--ttl HOURS]", file=sys.stderr)
        return 2
    kind = sys.argv[1]
    dir_ = sys.argv[2]
    target = sys.argv[3]
    ttl = 720 if kind == "intel" else 168
    # 解析 --ttl
    if "--ttl" in sys.argv:
        i = sys.argv.index("--ttl")
        ttl = int(sys.argv[i + 1])
    if kind == "intel":
        r = check_intel_cache(dir_, target, ttl)
    elif kind == "contract":
        version = sys.argv[4]
        r = check_contract_cache(dir_, target, version, ttl)
    else:
        print(f"未知 kind: {kind}", file=sys.stderr)
        return 2
    exit_map = {CacheStatus.USABLE: 0, CacheStatus.STALE: 1, CacheStatus.INVALID: 2,
                CacheStatus.MISMATCH: 3, CacheStatus.MISSING: 4}
    print(f"{kind} cache: {r.status.value} — {r.reason} ({r.path})")
    return exit_map[r.status]


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `PYTHONUTF8=1 python -m pytest tests/test_check_cache.py -v`
Expected: 7 passed。

- [ ] **Step 5: Commit**

```bash
git add scripts/check_cache.py tests/test_check_cache.py
git commit -m "feat(cache): check_cache.py D 判断（缓存+TTL+有效性+target/version）+ TDD (批次 D Task 1)"
```

---

## Task 2: commands/contract.md — 契约生成 SOP

**Files:**
- Create: `commands/contract.md`

提取自 `mine.md` Step 4（knowledge-extractor）+ Step 5（contract-formalizer）+ Step 6（合同门控），加 `--force` 参数。

- [ ] **Step 1: 写 contract.md**

Create `commands/contract.md`，内容结构（从 mine.md Step 4-6 提取 + 参数化）：

```markdown
---
description: 单独生成/刷新指定 DB 版本的文档知识与结构化契约
---

# /testvdb:contract — 文档提取 + 契约生成

## Usage
/testvdb:contract <db> <version> [--force]

## 参数
- <db>: milvus | qdrant | weaviate | pgvector
- <version>: 目标版本（如 1.38.0）
- --force: 强制重新生成（忽略缓存）

## 行为
1. 解析参数 + 前置检查（Docker/Python/network）
2. 缓存检查：`results/<db>/<version>/structured_contract.json` 存在且未过期（TTL=knowledge.cache_ttl_hours，默认168h）且有效 → 除非 --force，否则跳过生成（报告"缓存有效"）
3. **Step A: 派 Knowledge Extractor**（提取自 mine.md Step 4）：派发 testvdb:knowledge-extractor agent，从官方文档提取 raw_knowledge.md → `results/<db>/<version>/raw_knowledge.md`
4. **Step B: 派 Contract Formalizer**（提取自 mine.md Step 5）：派发 testvdb:contract-formalizer agent，raw_knowledge → structured_contract.json
5. **Step C: 合同门控**（提取自 mine.md Step 6）：validate_contract.py + passport_verify.py 验证契约合法性
6. 输出：`results/<db>/<version>/structured_contract.json` + 报告端点数/category 分布

## 独立性
本命令**只跑文档+契约**，不启动攻击/执行/judge/reporting。用于调试 contract-formalizer、验证契约（如 bug #3 category）、刷新过期契约。
```

> **实现者注意**：Step A/B/C 的详细 agent 派发 prompt + 验证脚本调用，**从 `commands/mine.md` 的 Step 4/5/6 原文逐字提取**（保持 SOP 一致）。contract.md 是独立命令，但 agent 派发逻辑与 mine.md 原Step 4-6 相同。

- [ ] **Step 2: grep 验证**

Run: `grep -c "contract-formalizer\|knowledge-extractor\|--force\|structured_contract" commands/contract.md`
Expected: ≥4（含关键 agent + 参数 + 输出）。

- [ ] **Step 3: Commit**

```bash
git add commands/contract.md
git commit -m "feat(commands): /testvdb:contract 独立契约生成命令 (批次 D Task 2)"
```

---

## Task 3: commands/intel.md — 情报采集 SOP

**Files:**
- Create: `commands/intel.md`

提取自 `mine.md` Step 3.6（3.6a-3.6e：情报缓存 + issue-miner + bug-shape + threat-modeler），加 `--max-issues`/`--max-commits` 参数。

- [ ] **Step 1: 写 intel.md**

Create `commands/intel.md`，结构（从 mine.md Step 3.6 提取 + 参数化）：

```markdown
---
description: 单独采集指定 DB 的历史 Issue/Commit 情报并构建威胁模型
---

# /testvdb:intel — 情报采集 + 威胁建模

## Usage
/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]

## 参数
- <db>: milvus | qdrant | weaviate | pgvector
- --max-issues N: 采集最近 N 条 issue（默认读 settings.json intelligence.max_issues）
- --max-commits N: 采集最近 N 个 commit（默认 intelligence.max_commits）
- --force: 强制重新采集（忽略缓存）

## 行为
1. 解析参数 + 前置检查
2. 缓存检查：`intelligence/<db>/threat_model.json` 存在且未过期（TTL=intelligence.cache_ttl_hours，默认720h=30天）→ 除非 --force，否则跳过
3. **Step A: 派 issue-miner**（提取自 mine.md 3.6b）：爬取目标仓库最近 --max-issues 条 issue + 已合并 PR
4. **Step B: 派 bug-shape-extractor**（提取自 mine.md 3.6c）：对 issue 三分类，提取根因模式
5. **Step C: 派 threat-modeler**（提取自 mine.md 3.6d）：构建威胁模型 + 认知盲点
6. 输出：`intelligence/<db>/threat_model.json` + 报告盲点数/优先攻击面

## 独立性
本命令**只跑情报**，不跑契约/攻击/执行。用于刷新过期情报、跨 DB 迁移前更新威胁模型、单独调试 threat-modeler。
```

> **实现者注意**：Step A/B/C 的 agent 派发 prompt 从 `mine.md` 3.6b/3.6c/3.6d 原文提取。--max-issues/--max-commits 覆盖 settings.json 默认值（传参给 issue-miner agent prompt）。

- [ ] **Step 2: grep 验证**

Run: `grep -c "issue-miner\|threat-modeler\|bug-shape\|--max-issues\|threat_model" commands/intel.md`
Expected: ≥4。

- [ ] **Step 3: Commit**

```bash
git add commands/intel.md
git commit -m "feat(commands): /testvdb:intel 独立情报采集命令 (批次 D Task 3)"
```

---

## Task 4: plugin.json 注册 intel/contract 命令

**Files:**
- Modify: `.claude-plugin/plugin.json`

- [ ] **Step 1: 注册命令**

Edit `.claude-plugin/plugin.json`，`commands` 数组加 intel.md/contract.md：

old:
```json
  "commands": [
    "./commands/mine.md"
  ],
```
new:
```json
  "commands": [
    "./commands/mine.md",
    "./commands/contract.md",
    "./commands/intel.md"
  ],
```

- [ ] **Step 2: 验证 JSON 合法**

Run: `python -c "import json; json.load(open('.claude-plugin/plugin.json')); print('JSON OK')"`
Expected: `JSON OK`

- [ ] **Step 3: Commit**

```bash
git add .claude-plugin/plugin.json
git commit -m "feat(plugin): register /testvdb:intel and /testvdb:contract commands (批次 D Task 4)"
```

---

## Task 5: mine.md 重构 — 智能消费 + D 判断 + 参数 + C 边界

**Files:**
- Modify: `commands/mine.md`（核心重构）

这是批次 D 的核心——mine.md 从"全流程"重构为"智能消费 intel/contract + 挖掘编排"。

- [ ] **Step 1: Parameters 加 --intel/--contract**

Edit `commands/mine.md` 的 Parameters section（约 49-58 行），加：

```markdown
| --intel true\|false | 否 | auto | true=强制重新采集情报；false=禁用情报生成（无情报报错，有过期用+警告，C 边界） |
| --contract true\|false | 否 | auto | true=强制重新生成契约；false=禁用契约生成（C 边界） |
```

- [ ] **Step 2: Step 3.6（情报）改为智能消费**

Edit mine.md Step 3.6（约 162-217 行），改为：

```markdown
### Step 3.6: 情报阶段（智能消费，批次 D）

**智能判断（不传 --intel 时，D 判断）**：
python scripts/check_cache.py intel intelligence/{target} {target} --ttl {intel_ttl}
- USABLE → 跳过情报采集，直接 3.6e 加载（纯挖掘）
- MISSING/STALE/INVALID → 派发情报采集（3.6b-3.6d，逻辑同 /testvdb:intel 命令）
- MISMATCH → 报错（target 不匹配）

**--intel true**：跳过 check_cache，强制派发 3.6b-3.6d
**--intel false（C 边界）**：
  - MISSING → 报错退出（"情报缺失，--intel false 跳过采集；请先 /testvdb:intel {target}"）
  - STALE/INVALID → 用现有 + 警告（"情报可能过期，--intel false 跳过刷新"）
  - USABLE → 正常用

（3.6b-3.6e 的 agent 派发逻辑保持不变——提取自原 Step 3.6，参考 commands/intel.md）
```

- [ ] **Step 3: Step 4-6（契约）改为智能消费**

Edit mine.md Step 3（缓存检查，147）+ Step 4-6（218-238），改为智能消费：

```markdown
### Step 3: 契约阶段智能消费（批次 D，合并原 Step 3/4/5/6）

**智能判断（不传 --contract 时，D 判断）**：
python scripts/check_cache.py contract results/{target}/{version} {target} {version} --ttl {contract_ttl}
- USABLE → 跳过契约生成，直接 Step 7（纯挖掘）
- MISSING/STALE/INVALID → 派发契约生成（Step A knowledge-extractor + Step B contract-formalizer + Step C 门控，逻辑同 /testvdb:contract 命令）
- MISMATCH → 报错

**--contract true**：强制派发 Step A-C
**--contract false（C 边界）**：
  - MISSING → 报错退出（"契约缺失，--contract false 跳过生成；请先 /testvdb:contract {target} {version}"）
  - STALE/INVALID → 用现有 + 警告
  - USABLE → 正常用

（Step A-C 的 agent 派发保持不变——提取自原 Step 4/5/6，参考 commands/contract.md）
```

- [ ] **Step 4: Step 8（挖掘）保留不变**

挖掘循环（8a-8j）不动——它是 mine 的核心，消费 intel/contract 后执行。

- [ ] **Step 5: grep 验证**

Run:
```bash
grep -c "check_cache\|--intel\|--contract\|C 边界\|智能消费" commands/mine.md
```
Expected: ≥5（含 check_cache 引用 + 参数 + C 边界 + 智能消费）。

- [ ] **Step 6: 验证 mine.md 行数（应下降——提取了情报/契约）**

Run: `wc -l commands/mine.md`
Expected: < 763（提取了 Step 3.6/4-6 的详细 agent prompt 到 intel.md/contract.md，mine.md 变薄）。

- [ ] **Step 7: Commit**

```bash
git add commands/mine.md
git commit -m "refactor(mine): 智能消费 intel/contract + D 判断 + --intel/--contract 参数 + C 边界 (批次 D Task 5)"
```

---

## Task 6: orchestrator.md 瘦化 + 全局验收

**Files:**
- Modify: `agents/orchestrator.md`（顺带瘦化，834 行 → <800）

- [ ] **Step 1: orchestrator.md 瘦化**

mine.md 重构后，orchestrator.md 引用的"情报/契约阶段"逻辑移到 intel/contract 命令。提取 orchestrator.md 中重复的情报/契约编排细节（如果有的话）到 intel.md/contract.md 引用，或精简。目标：<800 行。

（具体提取点：grep orchestrator.md 的 "intelligence\|contract\|knowledge-extractor\|issue-miner" —— 如有重复 mine.md 的编排，提取/精简）

- [ ] **Step 2: 行数验证**

Run: `wc -l agents/orchestrator.md`
Expected: < 800。

- [ ] **Step 3: 全局 pytest（不回归）**

Run: `PYTHONUTF8=1 python -m pytest tests/`
Expected: 全 pass（48 + check_cache 7 = 55）。

- [ ] **Step 4: grep 全局验证命令注册 + 引用一致**

Run:
```bash
grep "contract.md\|intel.md" .claude-plugin/plugin.json   # 注册
grep -c "check_cache\|/testvdb:contract\|/testvdb:intel" commands/mine.md  # mine 引用
```
Expected: plugin.json 含 contract.md/intel.md；mine.md 引用 check_cache + 命令。

- [ ] **Step 5: Commit + 收尾**

```bash
git add agents/orchestrator.md
git commit -m "refactor(orchestrator): 瘦化 (<800行) — 情报/契约编排移至 intel/contract 命令 (批次 D Task 6)"
```

---

## 端到端验证（合并后，需 CC 2.1.165）

- [ ] `/testvdb:contract weaviate 1.38.0` 单独跑 → 生成契约，不跑挖掘
- [ ] `/testvdb:intel weaviate --max-issues 50` 单独跑 → 生成情报
- [ ] `/testvdb:mine weaviate 1.38.0`（缓存全有效）→ 纯挖掘（日志 "intel: cached, contract: cached"）
- [ ] `/testvdb:mine weaviate 1.38.0 --contract true` → 强制重生成契约
- [ ] `/testvdb:mine weaviate 1.38.0 --contract false`（无契约）→ 报错（C 边界）

---

## Self-Review

**Spec coverage**：
- 三命令（contract/intel/mine）→ Task 2/3/5 ✓
- mine 智能 D 判断 → Task 1（check_cache）+ Task 5（mine 引用）✓
- 参数 --intel/--contract → Task 5 ✓
- C 边界 → Task 5（mine.md C 边界逻辑）✓
- 向后兼容 → Task 5（mine 默认智能 = 旧行为升级）✓
- plugin.json 注册 → Task 4 ✓
- orchestrator 瘦化 → Task 6 ✓

**Placeholder scan**：无 TBD。contract.md/intel.md 的 agent 派发指引"从 mine.md 提取"（精确 Step 号）——这是可执行的提取指引，非 placeholder。

**Type consistency**：check_cache 的 CacheStatus（USABLE/MISSING/STALE/INVALID/MISMATCH）在 Task 1 定义，Task 5（mine.md）引用一致的 `check_cache.py intel/contract ...` 命令 + 状态名。✓
