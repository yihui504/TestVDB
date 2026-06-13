# 批次 A · 攻击模板去 DB 硬编码 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 TestVDB 攻击 Agent 真正 target 中立——从源头注入正确端点、模板去 Qdrant/weaviate 硬编码、新增 target-aware 验证器、修复 gate 空声明绕过。

**Architecture:** 三层防御——源头（reconstruct_context 注入契约端点速查表）→ 模板（三个 attack agent 示例改契约占位符 + safe_request 统一）→ 强制（validate_target_neutrality.py 在 Stage 1 REJECT 文不对题脚本 + gate 兜底空声明）。生成层纯契约驱动，检测层用最小签名表做 target-aware 启发式。

**Tech Stack:** Python 3.12（脚本）、Markdown（agent 模板）、独立 `_test_*.py` 测试脚本（项目约定，非 pytest）、subprocess exit-code 断言。

**Spec:** `docs/superpowers/specs/2026-06-13-batch-a-target-neutralization-design.md`

---

## File Structure

| 文件 | 操作 | 责任 |
|------|------|------|
| `scripts/reconstruct_context.py` | 修改 | 提取 + 输出契约端点速查表（组件 C） |
| `scripts/_test_reconstruct_context.py` | 新建 | 组件 C 测试（自造 fixture） |
| `agents/_target_api_reference.md` | 修改 | 新增 safe_request 权威定义（组件 #1a） |
| `agents/attack-boundary.md` | 修改 | 示例去 Qdrant 硬编码（组件 #1） |
| `agents/attack-state.md` | 修改 | 示例去 Qdrant 硬编码（组件 #1） |
| `agents/attack-semantic.md` | 修改 | 示例去 Qdrant 硬编码（组件 #1） |
| `scripts/validate_target_neutrality.py` | 新建 | target-aware 签名检测器（组件 B） |
| `scripts/_test_validate_target_neutrality.py` | 新建 | 组件 B 测试 |
| `commands/mine.md` | 修改 | Stage 1 接入验证器（组件 B 集成） |
| `scripts/hooks/pipeline_gate.py` | 修改 | check_doc_coverage 空声明拦截（组件 #2'） |
| `scripts/hooks/_test_pipeline_gate.py` | 修改 | 新增空声明场景（组件 #2' 测试） |

**测试约定**（遵循项目既有模式，参照 `_test_pipeline_gate.py`）：
- 不引入 pytest。每个 `_test_*.py` 自带 `PASSED/FAILED` 列表 + `main()` 返回 `0`（全过）/ `1`（有失败）。
- fixture 用 `tempfile.mkdtemp()` 自造临时 session tree，**绝不依赖 `results/`**（被 gitignore）。
- 跑真实脚本 via `subprocess.run([sys.executable, script])`，断言 `returncode`。

---

## 范式：target 中立示例代码（Task 3/4/5 共用）

三个 attack agent 的所有策略示例统一改用此范式。**核心：占位符变量 + 注释指明从速查表/contract 取值，代码不出现任何具体 DB 的端口/路径/字段/响应键。**

```python
# === 契约驱动：以下变量从注入的「端点速查表」+ structured_contract.json 取当前 target 的实际值 ===
# ⛔ 禁止硬编码端口(6333/8080/19530)、路径(/collections/.../points)、字段(payload/properties)、
#    响应键(result)——它们随 target 变化。从速查表/contract 读，或用占位符。
SEARCH_PATH = "<从速查表取当前 target 的 search 端点 path>"   # 例：速查表里 category=search 的 path
VECTOR_KEY  = "<从 contract.data_types 取向量字段名>"          # qdrant=vector, weaviate=vector, milvus 按 schema
DIM         = 128   # 从 contract 取实际维度；此处仅示例数值

status, body, raw = safe_request("POST", SEARCH_PATH,
    json={VECTOR_KEY: [0.1] * DIM, "limit": 0})
print(raw)                       # 先打印原始响应，HTTP status 为主判定
if status not in (400, 422):
    print(f"VERDICT: DEFECT_FOUND (Type1_IllegalSuccess) — limit=0 应被拒绝，got {status}")
    sys.exit(1)
```

判定原则：**HTTP `status_code` 为主 + `print(raw_text)`**；响应体解析按 `contract.target` 动态选键，不假设固定结构（如不写死 `body["result"]`）。

---

## Task 1: 组件 C — reconstruct_context 端点速查表注入

**Files:**
- Modify: `scripts/reconstruct_context.py`（`reconstruct()` 第 161-170 行 + `format_text()` 插入新 section）
- Test: `scripts/_test_reconstruct_context.py`

- [ ] **Step 1: 写失败测试**

Create `scripts/_test_reconstruct_context.py`:

```python
#!/usr/bin/env python3
"""TestVDB 组件 C 测试 — reconstruct_context 端点速查表注入。

自造临时 session tree（不依赖 results/，因其被 gitignore），调用真实
reconstruct() + format_text()，断言 target_reference 与速查表 section。
"""
from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from reconstruct_context import reconstruct, format_text  # noqa: E402

PASSED: list[str] = []
FAILED: list[str] = []


def _check(name: str, cond: bool, detail: str = "") -> None:
    (PASSED if cond else FAILED).append(name)
    print(f"[{'PASS' if cond else 'FAIL'}] {name}" + (f" — {detail}" if detail and not cond else ""))


def _scaffold() -> Path:
    """造临时 session tree：root/results/testdb/1.0/，含 pipeline_state + structured_contract。"""
    root = Path(tempfile.mkdtemp(prefix="recon_"))
    ver_dir = root / "results" / "testdb" / "1.0"
    ver_dir.mkdir(parents=True)

    pipeline_state = {
        "version": 3,
        "session_id": "test-001",
        "target": "testdb",
        "version_target": "1.0",
        "current_round": 1,
        "max_rounds": 1,
        "phase": "ATTACK_GEN",
        "phases_completed": ["ROUND_START"],
        "project_root": str(root),
        "session_dir": "results/testdb/1.0",
        "global_state": {"total_defects_confirmed": 0, "consecutive_no_defect_rounds": 0,
                         "docker_container_running": True},
    }
    (ver_dir / "pipeline_state.json").write_text(json.dumps(pipeline_state), encoding="utf-8")

    contract = {
        "target": "testdb",
        "version": "1.0",
        "api_endpoints": [
            {"path": "collections+{collection_name}", "method": "PUT", "category": "collections",
             "source_url": "https://example.test/docs/create", "parameters": []},
            {"path": "collections+{collection_name}+points+search", "method": "POST",
             "category": "search", "source_url": "https://example.test/docs/search", "parameters": []},
            {"path": "collections+{collection_name}+points", "method": "PUT", "category": "points",
             "source_url": "https://example.test/docs/upsert", "parameters": []},
        ],
        "data_types": [{"name": "vector", "type": "array"}],
    }
    (ver_dir / "structured_contract.json").write_text(json.dumps(contract), encoding="utf-8")
    return root


def main() -> int:
    root = _scaffold()
    try:
        session_dir = str(root / "results" / "testdb" / "1.0")
        data = reconstruct(session_dir)

        # 1. target_reference 存在
        tr = data.get("target_reference")
        _check("1 target_reference 存在", tr is not None)

        # 2. target 正确
        _check("2 target=testdb", tr is not None and tr.get("target") == "testdb")

        # 3. endpoint_cheatsheet 非空且含全部 3 个端点
        cs = tr.get("endpoint_cheatsheet", []) if tr else []
        _check("3 cheatsheet 含 3 端点", len(cs) == 3, f"got {len(cs)}")
        if cs:
            _check("3b cheatsheet 条目含 method/path/category",
                   all({"method", "path", "category"} <= set(e.keys()) for e in cs))

        # 4. key_data_types 透传
        _check("4 key_data_types 透传",
               tr is not None and len(tr.get("key_data_types", [])) == 1)

        # 5. format_text 输出含速查表 section
        text = format_text(data)
        _check("5 format_text 含『端点速查表』section", "端点速查表" in text)
        _check("6 format_text 含 target 标注", "Target: testdb" in text or "testdb" in text)
        _check("7 format_text 含端点 markdown 表", "| Method |" in text or "Method" in text)
        # 速查表 section 不应泄漏 description/parameters（精简）
        _check("8 速查表 section 在「本轮关键信息」之前",
               text.index("端点速查表") < text.index("本轮关键信息"))
    finally:
        shutil.rmtree(root, ignore_errors=True)

    print(f"\n{len(PASSED)}/{len(PASSED) + len(FAILED)} passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd scripts && python _test_reconstruct_context.py`
Expected: FAIL — `target_reference` 不存在（当前 reconstruct 不产出它），多条 FAIL，`0/N passed`。

- [ ] **Step 3: 实现 reconstruct() 的 target_reference 提取**

Modify `scripts/reconstruct_context.py` 第 161-170 行。找到现有块：

```python
    # 5. structured_contract.json (summary only)
    contract_path = os.path.join(session_dir, "structured_contract.json")
    contract = _read_json(contract_path)
    endpoint_count = 0
    constraint_count = 0
    if contract:
        endpoints = contract.get("api_endpoints", [])
        endpoint_count = len(endpoints)
        for ep in endpoints:
            constraint_count += len(ep.get("constraints", []))
```

替换为：

```python
    # 5. structured_contract.json — summary + target_reference 速查表（组件 C）
    contract_path = os.path.join(session_dir, "structured_contract.json")
    contract = _read_json(contract_path)
    endpoint_count = 0
    constraint_count = 0
    endpoint_cheatsheet: list[dict[str, str]] = []
    if contract:
        endpoints = contract.get("api_endpoints", [])
        endpoint_count = len(endpoints)
        for ep in endpoints:
            constraint_count += len(ep.get("constraints", []))
        endpoint_cheatsheet = [
            {
                "method": str(ep.get("method", "")),
                "path": str(ep.get("path", "")),
                "category": str(ep.get("category", "")),
            }
            for ep in endpoints
            if isinstance(ep, dict)
        ]

    # target_reference：契约端点速查表，注入 attack agent prompt 供生成脚本引用
    result["target_reference"] = {
        "target": str(target),
        "endpoint_cheatsheet": endpoint_cheatsheet,
        "key_data_types": contract.get("data_types", []) if contract else [],
    }
```

- [ ] **Step 4: 实现 format_text() 速查表 section**

Modify `scripts/reconstruct_context.py` 的 `format_text()`。找到 `lines.extend(["", "### 本轮关键信息",])`（约第 331 行），在其**之前**插入新 section：

```python
    # 组件 C：端点速查表 section（注入 attack agent，供生成脚本引用）
    tr = data.get("target_reference", {})
    cheatsheet = tr.get("endpoint_cheatsheet", [])
    if cheatsheet:
        lines.extend([
            "",
            f"### 当前 Target 端点速查表（契约驱动——生成脚本时引用此表，禁止硬编码端口/路径）",
            f"- Target: {tr.get('target', '?')}  |  端点数: {len(cheatsheet)}",
            "| Method | Path | Category |",
            "|--------|------|----------|",
        ])
        for ep in cheatsheet[:40]:  # 上限 40 条防 token 爆炸；其余 agent 自行读 contract
            lines.append(f"| {ep.get('method','')} | {ep.get('path','')} | {ep.get('category','')} |")
        if len(cheatsheet) > 40:
            lines.append(f"| ... | (另 {len(cheatsheet)-40} 条见 structured_contract.json) | ... |")
        lines.append("- 数据字段命名/向量格式: 见 contract.data_types（key_data_types 已注入）")
        lines.append("- ⛔ 禁止写死端口(6333/8080/19530)、路径、payload/properties 字段名——一律从本表或 contract 推导")
```

- [ ] **Step 5: 运行测试，确认通过**

Run: `cd scripts && python _test_reconstruct_context.py`
Expected: PASS — 全部断言通过，`8/8 passed`。

- [ ] **Step 6: Commit**

```bash
git add scripts/reconstruct_context.py scripts/_test_reconstruct_context.py
git commit -m "feat(contract): inject target endpoint cheatsheet into attack agent context (组件 C)"
```

---

## Task 2: 组件 #1a — safe_request 权威定义统一

**Files:**
- Modify: `agents/_target_api_reference.md`（新增权威定义 section）
- Modify: `agents/attack-boundary.md` / `attack-state.md` / `attack-semantic.md`（输出格式引用化）

- [ ] **Step 1: 在 _target_api_reference.md 新增 safe_request 权威定义**

在 `agents/_target_api_reference.md` 末尾（「参考样板」section 之前）插入新 section：

```markdown
## safe_request 权威定义（三 attack agent 共用）

所有攻击脚本的 HTTP 调用**必须**用此包装器。返回三元组 `(status_code, body_or_None, raw_text)`。
三个 attack agent 的「输出格式」section 引用本定义，不再各自重写。

模块级变量来源：
- `BASE_URL = os.environ.get("TESTVDB_DB_URL")` —— 由 docker-executor 设置正确端口；**无默认端口**，缺失则打印 `VERDICT: SCRIPT_ERROR` 退出。
- `AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")` —— 可选鉴权头。

```python
import requests, json, sys, os

BASE_URL = os.environ.get("TESTVDB_DB_URL")
if not BASE_URL:
    print("VERDICT: SCRIPT_ERROR — TESTVDB_DB_URL not set (see agents/_target_api_reference.md)")
    sys.exit(2)
AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")

def safe_request(method, path, **kwargs):
    """Resilient HTTP wrapper. Returns (status_code, body_or_None, raw_text).
    连接失败: 打印 REQUEST_ERROR, 返回 (0, None, "")。
    JSON 解析失败: 打印 JSON_DECODE_ERROR, 返回 (status, None, text)。"""
    url = f"{BASE_URL}{path}"
    headers = kwargs.pop("headers", {"Content-Type": "application/json"})
    if AUTH_HEADER:
        headers["Authorization"] = AUTH_HEADER
    try:
        resp = requests.request(method, url, headers=headers, timeout=30, **kwargs)
        status = resp.status_code
        text = resp.text
        try:
            body = resp.json() if text else {}
        except (json.JSONDecodeError, ValueError):
            print(f"JSON_DECODE_ERROR: {text[:200]}")
            return status, None, text
        return status, body, text
    except requests.exceptions.RequestException as e:
        print(f"REQUEST_ERROR: {e}")
        return 0, None, ""
```

调用示例（判定以 HTTP `status` 为主 + `print(raw)`）：
```python
status, body, raw = safe_request("POST", "<cheatsheet search path>",
                                  json={"<vector field>": [0.1]*128, "limit": 0})
print(raw)
if status == 0:
    print("VERDICT: SCRIPT_ERROR — connection failed"); sys.exit(2)
if status not in (400, 422):
    print(f"VERDICT: DEFECT_FOUND (Type1_IllegalSuccess) — got {status}"); sys.exit(1)
print("VERDICT: NO_DEFECT")
```
```

- [ ] **Step 2: 三个 agent 输出格式 section 改为引用 + 统一三元组**

对 `attack-boundary.md`，找到「输出格式」section 内的 `def safe_request(...)` 定义块（约第 224-247 行）+ `def test_boundary()` 内的 `status, body, raw = safe_request(...)`（已是三元组，保留）。

将 boundary 的独立 `def safe_request` 定义块（第 223-247 行整段，从 `# ⛔ ALL HTTP calls...` 注释到 `return 0, None, ""`）**删除**，替换为：

```markdown
**⛔ 脚本格式强制要求：每个生成的脚本必须使用 `safe_request()` 包装所有 HTTP 调用。**

- `safe_request()` 权威定义（含 `BASE_URL`/`AUTH_HEADER` 来源）见 `agents/_target_api_reference.md`。
- 返回三元组 `(status, body, raw_text)`；判定以 HTTP `status` 为主 + `print(raw)`。
- 裸 `requests.post(url, json=...).json()` 链式调用 → 流水线 REJECT。
- 脚本末尾必须打印 `VERDICT: DEFECT_FOUND` / `NO_DEFECT` / `SCRIPT_ERROR`。
```

对 `attack-state.md`，找到「输出格式」section（约第 256-278 行）+「脚本健壮性要求」section（约第 308-342 行）的**两处** `def safe_request` 定义。两处都返回二元组 `(status, body)`。

将这两处的 `def safe_request` 定义 + 调用示例删除/替换为引用（同上 boundary 的替换文本）。同时把「输出格式」「健壮性」**这两 section 内**的 `status, body = safe_request(...)` 调用改为 `status, body, raw = safe_request(...)`（策略示例 section 里的调用留到 Task 4 一并改）。

对 `attack-semantic.md`，找到「输出格式」section（约第 355-373 行）的 `def safe_request` 定义（二元组）。删除定义替换为引用（同上）。把 semantic 内所有 `status, body = safe_request(...)` 调用改为 `status, body, raw = safe_request(...)`。

> 注：Task 3/4/5 改示例代码时会一并把示例里的 safe_request 调用统一为三元组；本 step 先消除重复定义。

- [ ] **Step 3: grep 验证**

Run:
```bash
# 权威定义只在 _target_api_reference.md 出现一次
grep -rl "def safe_request" agents/   # 期望: 仅 agents/_target_api_reference.md
# 三 agent 不再含 def safe_request
grep -c "def safe_request" agents/attack-boundary.md agents/attack-state.md agents/attack-semantic.md
# 期望: 每个文件 0
```
Expected: 第一个命令只输出 `_target_api_reference.md`；第二个命令三个文件都是 `0`。

- [ ] **Step 4: Commit**

```bash
git add agents/_target_api_reference.md agents/attack-boundary.md agents/attack-state.md agents/attack-semantic.md
git commit -m "refactor(agents): unify safe_request into _target_api_reference.md (组件 #1a)"
```

---

## Task 3: 组件 #1 — attack-boundary.md 示例去硬编码

**Files:**
- Modify: `agents/attack-boundary.md`（策略 1/3 示例 + 输出模板）

- [ ] **Step 1: 改策略 1 边界值示例（约第 121-133 行）**

找到这段（含 `localhost:6333`）：

```python
**生成示例**（limit 类参数，contract 要求 "limit > 0"）：
```python
# Test: limit = 0 (should be rejected)
response = requests.post(
    "http://localhost:6333/collections/{name}/points/search",
    json={"vector": [0.1]*128, "limit": 0}
)
if response.status_code not in (400, 422):
    print(f"VERDICT: DEFECT_FOUND (Type1_IllegalSuccess)")
    print(f"Expected 4xx, got {response.status_code}")
    sys.exit(1)
# Use explicit if-check, not assert (assert is stripped by python -O)
```
```

替换整个 ```python 代码块为（target 中立范式）：

```python
**生成示例**（limit 类参数，contract 要求 "limit > 0"）—— target 中立，套用 `_target_api_reference.md` 范式：
```python
# 契约驱动：端点/字段从注入速查表 + contract 取，禁止硬编码
SEARCH_PATH = "<速查表 category=search 的 path>"
VECTOR_KEY  = "<contract.data_types 的向量字段名>"
DIM         = 128  # 从 contract 取实际维度

status, body, raw = safe_request("POST", SEARCH_PATH,
    json={VECTOR_KEY: [0.1]*DIM, "limit": 0})
print(raw)
if status not in (400, 422):
    print(f"VERDICT: DEFECT_FOUND (Type1_IllegalSuccess) — limit=0 应被拒绝，got {status}")
    sys.exit(1)
# 用显式 if-check，不用 assert（assert 被 python -O 剥离）
```
```

- [ ] **Step 2: 改策略 3 维度示例（约第 155-166 行）**

找到这段（含 `/collections/test`、`"vectors":{"size":...,"distance":"Cosine"}`、`"points":[...]`）：

```python
### 策略 3: 维度不匹配攻击

针对向量维度参数：

```python
# Test: wrong dimension
response = requests.put(
    "http://localhost:6333/collections/test",
    json={"vectors": {"size": 128, "distance": "Cosine"}}
)
# Insert with wrong dimension
response = requests.put(
    "http://localhost:6333/collections/test/points",
    json={"points": [{"id": 1, "vector": [0.1]*64}]}  # 64 != 128
)
```
```

替换 ```python 代码块为：

```python
### 策略 3: 维度不匹配攻击

针对向量维度参数（target 中立：建集合 + 插入的路径/字段从速查表取）：

```python
CREATE_PATH = "<速查表 category=collections 的 path>"
UPSERT_PATH = "<速查表 category=points 的 path>"
COLLECTION_BODY = "<contract.data_types 推导的建集合体>"  # 含维度/距离配置
POINT_WRAP = "<contract.data_types 的点包装结构>"         # 如 {points:[...]} 或 {objects:[...]}

# 建集合（维度 DIM）
status, _, raw = safe_request("PUT", CREATE_PATH, json={COLLECTION_BODY: {"<dim field>": 128}})
# 插入错误维度（64 != 128）
status, _, raw = safe_request("PUT", UPSERT_PATH,
    json={POINT_WRAP: [{"id": 1, "vector": [0.1]*64}]})
print(raw)
```
```

- [ ] **Step 3: 改输出模板里的响应键（约第 273 行）**

找到输出模板 `test_boundary()` 里的：

```python
    # Type-2 check: error message quality
    if body and isinstance(body, dict):
        error_msg = body.get("status", {}).get("error", "") if isinstance(body.get("status"), dict) else ""
        if "limit" not in error_msg.lower():
```

替换为（不假设 Qdrant 的 `status.error` 结构，改为通用扫描 raw 文本）：

```python
    # Type-2 check: error message quality（不假设固定响应结构，扫 raw 文本）
    if "limit" not in raw.lower():
```

- [ ] **Step 4: grep 验证 boundary 无硬编码**

Run:
```bash
grep -nE '6333|/collections/|localhost' agents/attack-boundary.md
```
Expected: 仅可能在顶部契约驱动声明的禁令列表里出现 `6333`/`/collections/`（那是「禁止硬编码」的反面教材，允许）。策略示例代码区（第 100 行之后）应**无**裸 `6333`/`localhost`/`/collections/test`。人工确认示例代码区干净。

- [ ] **Step 5: Commit**

```bash
git add agents/attack-boundary.md
git commit -m "refactor(attack-boundary): de-qdrantize strategy examples to contract placeholders (组件 #1)"
```

---

## Task 4: 组件 #1 — attack-state.md 示例去硬编码

**Files:**
- Modify: `agents/attack-state.md`（策略 1/2/4 示例 + 健壮性示例）

- [ ] **Step 1: 改策略 1 COUNT 一致性示例（约第 110-125 行）**

找到（含 `f"{BASE_URL}/collections/test/points/count"`、`.json()["result"]["count"]`、`/collections/test/points`）：

```python
# Sequence: create → insert N → count = N
response = requests.get(f"{BASE_URL}/collections/test/points/count")
count_before = response.json()["result"]["count"]

# Insert M points
for i in range(M):
    requests.put(f"{BASE_URL}/collections/test/points",
                 json={"points": [{"id": i, "vector": [0.1]*128}]})

# Count should be count_before + M
response = requests.get(f"{BASE_URL}/collections/test/points/count")
count_after = response.json()["result"]["count"]
assert count_after == count_before + M, \
    f"StateLogicViolation: Expected {count_before + M}, got {count_after}"
```

替换为：

```python
# Sequence: create → insert N → count = N（target 中立：路径/字段/响应键从速查表+contract 取）
COUNT_PATH  = "<速查表 count 端点 path>"
UPSERT_PATH = "<速查表 points 端点 path>"
POINT_WRAP  = "<contract.data_types 点包装结构>"

_, body_before, _ = safe_request("GET", COUNT_PATH)
# count 从响应取：先 print(raw) 看结构，按 contract.target 选键，不假设 ["result"]["count"]
print(f"count_before raw: {body_before}")
count_before = "<从 body_before 按 target 动态取 count>"  # 实现时依据实际响应结构

for i in range(M):
    safe_request("PUT", UPSERT_PATH, json={POINT_WRAP: [{"id": i, "vector": [0.1]*128}]})

_, body_after, _ = safe_request("GET", COUNT_PATH)
count_after = "<从 body_after 按 target 动态取 count>"
if count_after != count_before + M:
    print(f"VERDICT: DEFECT_FOUND (Type4_StateLogicViolation) — Expected {count_before+M}, got {count_after}")
    sys.exit(1)
```

- [ ] **Step 2: 改策略 2 DELETE 示例（约第 133 行）+ 策略 4 并发示例（约第 159-191 行）**

策略 2 找到 `requests.get(f"{BASE_URL}/collections/deleted_collection/points/count")`，改为 `safe_request("GET", "<count path for 已删除集合>")`，`assert response.status_code == 404` 改为：
```python
status, _, raw = safe_request("GET", "<count path for 已删除集合>")
if status != 404:
    print(f"VERDICT: DEFECT_FOUND (Type4) — 已删集合应 404，got {status}"); sys.exit(1)
```

策略 4 并发示例找到 `requests.put(f"{BASE_URL}/collections/{collection}/points", json={"points": [...]})`（两处，约 166、187 行）+ `resp.json()["result"]["count"]`。改为：
- `requests.put(...)` → `safe_request("PUT", UPSERT_PATH, json={POINT_WRAP: [...]})`
- `resp.json()["result"]["count"]` → 先 `print(raw)`，count 按 target 动态取（同策略 1）

- [ ] **Step 3: 改健壮性示例响应键（约第 328-330 行）**

找到：

```python
# 使用示例
status, body = safe_request("GET", f"{BASE_URL}/collections/{name}")
if isinstance(body, dict):
    count = body.get("result", {}).get("count", -1)
```

替换为（消除 `f"{BASE_URL}/collections/{name}"` 硬编码路径 + `body.get("result")` 假设）：

```python
# 使用示例（target 中立）
status, body, raw = safe_request("GET", "<速查表 get-collection path>")
print(raw)  # 先看实际结构，按 contract.target 选键，不假设 ["result"]["count"]
```

- [ ] **Step 4: grep 验证**

Run: `grep -nE '6333|/collections/|\.json\(\)\["result' agents/attack-state.md`
Expected: 策略示例区无裸 `/collections/` 路径、无 `.json()["result"]` 链式调用（顶部禁令列表的提及允许）。

- [ ] **Step 5: Commit**

```bash
git add agents/attack-state.md
git commit -m "refactor(attack-state): de-qdrantize strategy examples to contract placeholders (组件 #1)"
```

---

## Task 5: 组件 #1 — attack-semantic.md 示例去硬编码

**Files:**
- Modify: `agents/attack-semantic.md`（策略 1/3/4/5/6/7 示例）

semantic 的示例最多（路径 `/collections/test/points/search` + `body["result"]` + filter `must/match`）。逐策略套用范式：

- [ ] **Step 1: 改策略 1 Behavioral Contract（约第 128-153 行）**

找到含 `safe_request("PUT", "/collections/test", ...)`、`"/collections/test/points"`、`"/collections/test/points/search"`、`body.get("result", [])` 的 behavioral 示例块。把所有路径改为占位符变量（`CREATE_PATH`/`UPSERT_PATH`/`SEARCH_PATH`，顶部声明从速查表取），`body.get("result", [])` 改为：
```python
# 结果数判定不假设 ["result"]：按 contract.target 选键或扫 raw
print(raw)
results = "<从 body 按 target 动态取结果列表>"
if isinstance(body, dict) and not results:
    print("VERDICT: DEFECT_FOUND (Type4_StateLogicViolation)"); sys.exit(1)
```

- [ ] **Step 2: 改策略 3/4（约第 199-243 行）**

策略 3（合法值测试）+ 策略 4（类型转换）的 `safe_request("POST", "/collections/test/points/search", json={"vector":[0.1]*128, "limit":...})` 多处。统一：路径用 `SEARCH_PATH` 占位符，`"vector"` 用 `VECTOR_KEY` 占位符。

- [ ] **Step 3: 改策略 5 搜索正确性（约第 250-283 行）**

找到 `safe_request("PUT", "/collections/test/points", json={"points":[...]})` + `body["result"]` + `results[0]["id"]`。路径改 `UPSERT_PATH`/`SEARCH_PATH`；`body["result"]` 改按 target 动态取 + `print(raw)`。

- [ ] **Step 4: 改策略 6 Metamorphic（约第 289-311 行）**

`safe_request("POST", "/collections/test/points/search", ...)` 两处 → `SEARCH_PATH`；`body1.get("result", [])` / `body2.get("result", [])` → 按 target 动态取结果列表。

- [ ] **Step 5: 改策略 7 过滤语义（约第 315-351 行）—— 关键：去 Qdrant filter 语法**

找到含 Qdrant 过滤语法的块：

```python
    # Filter by category "A"
    status, body = safe_request("POST", "/collections/test/points/search", json={
        "vector": [0.1]*128, "limit": 10,
        "filter": {"must": [{"key": "category", "match": {"value": "A"}}]}
    })
    ...
    status, body = safe_request("POST", "/collections/test/points/search", json={
        "vector": [0.1]*128, "limit": 10,
        "filter": {"must": [{"key": "score", "range": {"gt": 15}}]}
    })
```

替换为（filter 语法 target 化——**不写死 must/match**）：

```python
    # 过滤语法按 contract.target：qdrant={must:[{key,match}]}, weaviate={where:{operator,value}},
    # milvus={expr:"..."}, pgvector=SQL WHERE。从 contract.data_types/示例取当前 target 的写法。
    FILTER_EQ_A = "<contract 推导：当前 target 等值过滤 category=A 的语法>"
    FILTER_GT_15 = "<contract 推导：当前 target 范围过滤 score>15 的语法>"

    status, body, raw = safe_request("POST", SEARCH_PATH, json={
        VECTOR_KEY: [0.1]*128, "limit": 10, "filter": FILTER_EQ_A
    })
    print(raw)
    # 结果计数按 target 动态取，不假设 body["result"]
```

- [ ] **Step 6: grep 验证**

Run: `grep -nE '6333|/collections/|"must"|"match"' agents/attack-semantic.md`
Expected: 示例代码区无裸 `/collections/`、无 Qdrant `"must"`/`"match"` filter（顶部禁令列表提及允许）。

- [ ] **Step 7: Commit**

```bash
git add agents/attack-semantic.md
git commit -m "refactor(attack-semantic): de-qdrantize strategy examples + filter syntax (组件 #1)"
```

---

## Task 6: 组件 #3 — analyzed_documents 示例去 weaviate URL

**Files:**
- Modify: `agents/attack-boundary.md` / `attack-state.md` / `attack-semantic.md`（各「Analyzed Documents 产出契约」section）

- [ ] **Step 1: 替换三个 agent 的 analyzed_documents 示例**

三个 agent 的「### 输出格式」下都有相同的硬编码 weaviate URL 块（boundary 约 329-335、state 约 357-363、semantic 约 421-427）。各找到：

```markdown
### 输出格式

```markdown
## Analyzed Documents — boundary
- https://docs.weaviate.io/weaviate
- https://raw.githubusercontent.com/api-evangelist/weaviate/refs/heads/main/openapi/weaviate-openapi.yml
- https://github.com/weaviate/weaviate/releases/tag/v1.38.0
- https://pypi.org/pypi/weaviate-client/json
```
```

（state/semantic 的标题分别为 `— state` / `— semantic`）。

替换为（占位符 + 说明，三处仅 `— boundary/state/semantic` 不同）：

```markdown
### 输出格式

```markdown
## Analyzed Documents — boundary
- <逐字复制 raw_knowledge.md Document Sources 表第 1 行的 URL>
- <逐字复制第 2 行的 URL>
- <... 继续逐字复制，直到覆盖 ≥ 60% 的 Document Sources>
```

> ⚠️ 上方不提供具体 URL 示例——URL 随 target 变化（qdrant/weaviate/milvus/pgvector 各不同）。
> 照抄任何"看起来像"的 URL 会导致 gate 精确比对失败（覆盖率 0%）。
> 唯一正确做法：Read `raw_knowledge.md` → 找 `## Document Sources` 表 → 逐字复制 URL 列。
```

- [ ] **Step 2: grep 验证三 agent analyzed 示例无具体 URL**

Run:
```bash
grep -nE 'docs\.weaviate\.io|api-evangelist|weaviate-client/json' agents/attack-boundary.md agents/attack-state.md agents/attack-semantic.md
```
Expected: 无输出（三处示例都不再含具体 weaviate URL）。

- [ ] **Step 3: Commit**

```bash
git add agents/attack-boundary.md agents/attack-state.md agents/attack-semantic.md
git commit -m "refactor(agents): replace hardcoded weaviate URLs in analyzed_documents examples (组件 #3)"
```

---

## Task 7: 组件 B — validate_target_neutrality.py + Stage 1 集成

**Files:**
- Create: `scripts/validate_target_neutrality.py`
- Test: `scripts/_test_validate_target_neutrality.py`
- Modify: `commands/mine.md`（Stage 1 第 5 步后插入新步骤）

- [ ] **Step 1: 写失败测试**

Create `scripts/_test_validate_target_neutrality.py`:

```python
#!/usr/bin/env python3
"""TestVDB 组件 B 测试 — validate_target_neutrality.py target-aware 检测。

自造临时 session tree（structured_contract.json + 若干 fixture 脚本），
subprocess 跑真实验证器，断言 exit code。
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

VALIDATOR = Path(__file__).resolve().parent / "validate_target_neutrality.py"
PASSED: list[str] = []
FAILED: list[str] = []


def _check(name: str, got: int, want: int, out: str) -> None:
    ok = got == want
    (PASSED if ok else FAILED).append(name)
    print(f"[{'PASS' if ok else 'FAIL'}] {name}: exit={got} (want {want})")
    if not ok:
        print(f"    --- output ---\n{out}    --------------")


def _scaffold(target: str) -> Path:
    root = Path(tempfile.mkdtemp(prefix="neut_"))
    sd = root / "session"
    sd.mkdir()
    (sd / "structured_contract.json").write_text(
        json.dumps({"target": target, "api_endpoints": []}), encoding="utf-8"
    )
    return root, sd


def _run(sd: Path) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, str(VALIDATOR), str(sd)],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
    )
    return proc.returncode, proc.stdout + proc.stderr


# fixture 脚本
QDRANT_SIG = '''import requests
resp = requests.post("http://host:6333/collections/mycoll/points/search", json={})
x = resp.json()["result"]
'''
WEAVIATE_SIG = '''import requests
resp = requests.get("http://host:8080/v1/objects")
x = resp.json()["data"]
'''
CLEAN = '''import os, requests
url = os.environ["TESTVDB_DB_URL"]
resp = requests.post(url + "/some/path", json={})
'''


def main() -> int:
    # 1. target=weaviate 但脚本含 qdrant 签名(6333 + /collections/.../points + ["result"]) → REJECT(exit 1)
    root, sd = _scaffold("weaviate")
    try:
        (sd / "bad.py").write_text(QDRANT_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("1 weaviate+qdrant-sig → REJECT(1)", rc, 1, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 2. target=qdrant 脚本含 qdrant 签名 → PASS(exit 0)，不误伤
    root, sd = _scaffold("qdrant")
    try:
        (sd / "ok.py").write_text(QDRANT_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("2 qdrant+qdrant-sig → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 3. target=weaviate 脚本含 weaviate 签名 → PASS
    root, sd = _scaffold("weaviate")
    try:
        (sd / "ok.py").write_text(WEAVIATE_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("3 weaviate+weaviate-sig → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 4. target=weaviate 干净脚本(用 env url) → PASS
    root, sd = _scaffold("weaviate")
    try:
        (sd / "clean.py").write_text(CLEAN, encoding="utf-8")
        rc, out = _run(sd)
        _check("4 weaviate+clean → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 5. 端口误报控制：6333 出现在无关数字上下文(如 limit=6333) 不应触发(需 :6333 或 6333/)
    root, sd = _scaffold("weaviate")
    try:
        (sd / "num.py").write_text('x = 6333 + 1\nprint(x)\n', encoding="utf-8")
        rc, out = _run(sd)
        _check("5 裸数字 6333 非 URL 上下文 → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    print(f"\n{len(PASSED)}/{len(PASSED) + len(FAILED)} passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd scripts && python _test_validate_target_neutrality.py`
Expected: FAIL — `validate_target_neutrality.py` 不存在，subprocess 报 `FileNotFoundError` 或非 0 退出，多条 FAIL。

- [ ] **Step 3: 实现 validate_target_neutrality.py**

Create `scripts/validate_target_neutrality.py`:

```python
#!/usr/bin/env python3
"""Target neutrality validator — Stage 1 gate (组件 B).

DETECTION-ONLY signature table. NOT used for script generation.
See agents/_target_api_reference.md for the contract-driven generation principle.

Reads {session_dir}/structured_contract.json -> target, scans all generated *.py
for DB signatures that DON'T match the current target. target-aware:
qdrant syntax is legal when target=qdrant; REJECT only when mismatched
(e.g. target=weaviate but script hits :6333 / /collections/.../points).

Usage: python scripts/validate_target_neutrality.py <session_dir>
Exit: 0 = all pass / no foreign signatures; 1 = REJECT (foreign DB signatures found).
"""
from __future__ import annotations

import glob
import json
import os
import re
import sys

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


# DETECTION-ONLY — 不用于生成。高置信度稳定指纹（默认端口、核心路径前缀、过滤关键字、响应键访问语法）。
SIGNATURES: dict[str, dict[str, list[str]]] = {
    "qdrant": {
        "ports": [r":6333\b", r"\b6333/"],
        "paths": [r"/collections/[\w-]+/points(?:/search|/count)?",
                  r"/collections/\{[^}]+\}/points"],
        "filter_keys": [r'"must"\s*:', r"'must'\s*:", r'"match"\s*:'],
        "resp_keys": [r'\[\s*"result"\s*\]', r'\.get\(\s*"result"'],
    },
    "weaviate": {
        "ports": [r":8080\b", r"\b8080/"],
        "paths": [r"/v1/objects", r"/v1/schema", r'"/objects"', r'"/schema"'],
        "filter_keys": [r'"where"\s*:', r'"operator"\s*:'],
        "resp_keys": [r'\[\s*"data"\s*\]', r'\.get\(\s*"data"'],
    },
    "milvus": {
        "ports": [r":19530\b", r"\b19530/"],
        "paths": [r"/v2/vectordb/"],
        "filter_keys": [r'"expr"\s*:'],
        "resp_keys": [],
    },
    "pgvector": {
        "ports": [r":5432\b", r"\b5432/"],
        "paths": [],
        "filter_keys": [],
        "resp_keys": [],
    },
}


def _load_target(session_dir: str) -> str | None:
    path = os.path.join(session_dir, "structured_contract.json")
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        return str(data.get("target", "")).lower() or None
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None


def _scan_file(content: str) -> dict[str, list[str]]:
    """Return {db: [matched pattern strings]} for all DB signatures hit in content."""
    hits: dict[str, list[str]] = {}
    for db, groups in SIGNATURES.items():
        matched: list[str] = []
        for group_name in ("ports", "paths", "filter_keys", "resp_keys"):
            for pat in groups.get(group_name, []):
                if re.search(pat, content):
                    matched.append(f"{group_name}: {pat}")
        if matched:
            hits[db] = matched
    return hits


def validate(session_dir: str, target: str) -> list[dict]:
    """Return list of findings: scripts with foreign-DB signatures (not target)."""
    findings: list[dict] = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f.replace("\\", "/"):
            continue
        try:
            with open(f, encoding="utf-8", errors="replace") as fh:
                content = fh.read()
        except OSError:
            continue
        hits = _scan_file(content)
        # foreign = any DB signature that is NOT the current target
        foreign = {db: ev for db, ev in hits.items() if db != target}
        if foreign:
            findings.append({
                "file": os.path.relpath(f, session_dir),
                "foreign_dbs": foreign,
            })
    return findings


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python scripts/validate_target_neutrality.py <session_dir>", file=sys.stderr)
        return 2
    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        return 2

    target = _load_target(session_dir)
    if not target:
        print("[Stage 1] Target Neutrality: skipped (no target in structured_contract.json)")
        return 0

    findings = validate(session_dir, target)

    if findings:
        print(json.dumps({"target": target, "target_neutrality_violations": [
            {"file": x["file"], "foreign_dbs": x["foreign_dbs"]} for x in findings
        ]}, indent=2, ensure_ascii=False))
        print(f"[Stage 1] Target Neutrality Check: {len(findings)} script(s) REJECTED "
              f"(contain {target}-foreign DB signatures; target={target})")
        for x in findings:
            dbs = ", ".join(x["foreign_dbs"].keys())
            print(f"  REJECT: {x['file']} — foreign DB signature(s): {dbs}")
        return 1

    print(f"[Stage 1] Target Neutrality Check: all scripts consistent with target={target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd scripts && python _test_validate_target_neutrality.py`
Expected: PASS — `5/5 passed`。

- [ ] **Step 5: mine.md Stage 1 集成**

Modify `commands/mine.md`。找到第 408 行：

```
5. **API 调用格式 AST 验证**：`python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"`
6. 审查结果写入 `debate_logs/stage1.json`
7. 脚本路径标准化
```

在第 5 步之后插入新步骤，原 6/7 顺延：

```
5. **API 调用格式 AST 验证**：`python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"`
6. **Target 中立验证**：`python scripts/validate_target_neutrality.py "results/{target}/{version}/{timestamp}"`
   含与当前 target 不符的 DB 签名（如 target=weaviate 但脚本命中 :6333）的脚本 → 打回 Attack Agent 修改（同 8d.5 打回机制）。
7. 审查结果写入 `debate_logs/stage1.json`
8. 脚本路径标准化
```

- [ ] **Step 6: Commit**

```bash
git add scripts/validate_target_neutrality.py scripts/_test_validate_target_neutrality.py commands/mine.md
git commit -m "feat(validate): add target-aware neutrality validator + Stage 1 integration (组件 B)"
```

---

## Task 8: 组件 #2' — gate 空声明绕过修复

**Files:**
- Modify: `scripts/hooks/pipeline_gate.py`（`check_doc_coverage` 第 248-256 行）
- Modify: `scripts/hooks/_test_pipeline_gate.py`（_scaffold 支持 phases_completed + 新增场景）

- [ ] **Step 1: 扩展 _test_pipeline_gate.py 写失败测试**

Modify `scripts/hooks/_test_pipeline_gate.py`。

先给 `_scaffold` 加 `phases_completed` 支持。找到 `_scaffold` 的 `state = {...}`（约第 67-77 行），在 `"timestamp_dir": "20260612T100000",` 之后加一行：

```python
        "phases_completed": [],
```

并把 `_scaffold` 签名改为接受可选 phases：

```python
def _scaffold(phase: str, current: int, maxr: int, phases_completed: list[str] | None = None) -> tuple[Path, Path]:
```

在 `state = {...}` 内用 `"phases_completed": phases_completed or [],`。

然后在 `main()` 的 case 4 之后（约第 133 行后）插入新 case 4b：

```python
    # 4b. 空声明绕过: DONE + ATTACK_GEN completed + 无 analyzed_documents → exit 2（组件 #2'）
    root, _ = _scaffold("DONE", 1, 1, phases_completed=["ROUND_START", "ATTACK_GEN", "DEBATE_S1"])
    try:
        rc, out = _run_gate(root)
        _check("4b ① DONE+ATTACK_GEN+空analyzed → 2", rc, 2, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)
```

- [ ] **Step 2: 运行测试，确认新 case 失败（旧行为放行）**

Run: `cd scripts/hooks && python _test_pipeline_gate.py`
Expected: case `4b` FAIL（当前空 analyzed → exit 0，want 2）。其余 8 case 仍 PASS。

- [ ] **Step 3: 实现 check_doc_coverage 修改**

Modify `scripts/hooks/pipeline_gate.py` `check_doc_coverage`。找到（约第 248-256 行）：

```python
    analyzed = _parse_analyzed_docs(round_dir)
    if not analyzed:
        # No declarations yet: agents may not have run, or the round predates the
        # contract. Blocking here would misfire on legacy/partial rounds — only
        # block when we have *evidence* of incomplete coverage.
        log.warning(
            "doc-coverage: no analyzed_documents*.md in %s — cannot verify", round_dir
        )
        return True, "skipped (no analyzed_documents yet)"
```

替换为：

```python
    analyzed = _parse_analyzed_docs(round_dir)
    if not analyzed:
        # 区分"agent 还没跑"(放行) vs "agent 跑了但没写"(拦截空声明绕过)。
        # 判据: phase==DONE 且 ATTACK_GEN 已完成 → attack agent 跑过，analyzed 不应为空。
        attack_ran = (
            str(state.get("phase", "")).upper() == "DONE"
            and "ATTACK_GEN" in state.get("phases_completed", [])
        )
        if attack_ran:
            return False, (
                "Symptom ① — ATTACK_GEN completed but NO analyzed_documents written "
                "(空声明绕过). Attack agents must each emit analyzed_documents_*.md "
                "listing raw_knowledge.md Document Source URLs (see agents/attack-*.md)."
            )
        # agents 尚未跑或 round 早于合约 → 放行，避免误伤 legacy
        log.warning(
            "doc-coverage: no analyzed_documents*.md in %s — cannot verify", round_dir
        )
        return True, "skipped (no analyzed_documents yet — ATTACK_GEN not completed)"
```

- [ ] **Step 4: 运行测试，确认全通过**

Run: `cd scripts/hooks && python _test_pipeline_gate.py`
Expected: PASS — 全部 9 case 通过（含新 `4b`），`9/9 passed`。

- [ ] **Step 5: Commit**

```bash
git add scripts/hooks/pipeline_gate.py scripts/hooks/_test_pipeline_gate.py
git commit -m "fix(gate): block empty-statement bypass when ATTACK_GEN completed (组件 #2')"
```

---

## Task 9: 全局验证 + 收尾

- [ ] **Step 1: 跑全部新增/扩展测试**

Run:
```bash
cd scripts && python _test_reconstruct_context.py
python _test_validate_target_neutrality.py
cd hooks && python _test_pipeline_gate.py
```
Expected: 三个测试分别 `8/8`、`5/5`、`9/9` passed，退出码 0。

- [ ] **Step 2: grep 全局验证三 agent 模板无硬编码**

Run:
```bash
# 示例代码区无裸 qdrant 端口/路径（顶部禁令列表的提及不算）
grep -nE 'localhost:6333|/collections/test' agents/attack-boundary.md agents/attack-state.md agents/attack-semantic.md
# analyzed_documents 示例无具体 weaviate URL
grep -nE 'docs\.weaviate\.io|api-evangelist|weaviate-client/json' agents/attack-*.md
# safe_request 仅权威定义一处
grep -rl "def safe_request" agents/
```
Expected: 第一条——示例代码区干净（人工确认顶部禁令列表外无命中）；第二条——无输出；第三条——仅 `agents/_target_api_reference.md`。

- [ ] **Step 3: 更新验收清单文档**

在 `docs/acceptance-checklist-v2.1.1.md`（或新建 `docs/batch-a-acceptance.md`）追加批次 A 验收项，对应 spec 第 7 节 6 条验收标准，逐条标注实现 commit。

- [ ] **Step 4: Commit 收尾**

```bash
git add docs/
git commit -m "docs: batch A acceptance checklist + global verification"
```

- [ ] **Step 5: 端到端验证（可选，需 Docker）**

Run: `/testvdb:mine weaviate 1.38.0 --max-rounds 1`
Expected: 生成的脚本端口/路径来自 weaviate 契约（8080 + `/objects` 等），不含 Qdrant 的 6333/`/collections/.../points`；`validate_target_neutrality` 不 REJECT；gate 不因空声明放行。

---

## 验收标准对齐（spec 第 7 节）

| 验收项 | 实现任务 |
|--------|---------|
| 1. reconstruct 输出速查表 section | Task 1 |
| 2. 三 agent 示例零硬编码 | Task 3/4/5 + Task 6 |
| 3. safe_request 唯一权威定义 | Task 2 |
| 4. validate_target_neutrality + Stage 1 接入 | Task 7 |
| 5. gate 空 analyzed + ATTACK_GEN → exit 2 | Task 8 |
| 6. weaviate run 脚本不含 Qdrant 签名 | Task 9 Step 5 |
