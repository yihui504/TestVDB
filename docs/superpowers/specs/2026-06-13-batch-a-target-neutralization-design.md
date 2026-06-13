# 批次 A · 攻击模板去 DB 硬编码（Target Neutralization）设计

- **日期**: 2026-06-13
- **批次**: A（多 DB 正确性核心）
- **状态**: 设计已确认，待实现
- **作者**: 黄译辉
- **前置调研**: project memory `architecture` 记录 + v2.2/v2.3 code review 报告 + 源码审查

---

## 1. 背景与问题

TestVDB 的核心价值主张是"多向量数据库（Milvus/Qdrant/Weaviate/pgvector）合规性缺陷挖掘"。但当前攻击 Agent 体系存在一个架构级缺陷：**流水线实际是 Qdrant 中心化的**，声明与示例互相矛盾。

### 根因：声明 vs 示例分裂

三个攻击 Agent（`attack-boundary.md` / `attack-state.md` / `attack-semantic.md`）顶部都声明了契约驱动原则（"禁止硬编码任何 DB 特定值"），且 `agents/_target_api_reference.md` 规范写得完全正确。但**下方的策略示例代码和输出模板全部是 Qdrant 硬编码**。LLM Agent 生成脚本时倾向照抄具体示例而非遵循抽象禁令，导致：

- `target=weaviate` 时，Agent 照抄模板生成 `/collections/{x}/points/search` + 端口 `6333` 的脚本 → 命中 Qdrant API 而非 Weaviate → 假阴性
- `analyzed_documents` 示例硬编码 weaviate URL（`docs.weaviate.io/...`），Agent 照抄后 gate 做精确字符串比对 → 非 weaviate target 覆盖率 = 0%

代码审查（v2.2/v2.3）**未识别此层**——它们修了 `safe_request` 包装问题（H-N01/M-N08），但没碰 Qdrant 硬编码这个更深的根因。

### 连带问题

- **safe_request 三份定义不一致**：boundary 返回 `(status, body, text)` 三元组（带 REQUEST_ERROR/JSON_DECODE_ERROR 标记），state/semantic 返回 `(status, body)` 二元组。调用契约不统一。
- **gate 空声明绕过**：`pipeline_gate.py` 的 `check_doc_coverage` 在 `analyzed_documents` 为空时直接放行（设计取舍：避免误伤 legacy）。审计日志显示 `weaviate-1380-v2` session 在 ATTACK_GEN 被拦一次后，1 分钟内 `phase=DONE / all gates passed`——Agent 可通过"不写 analyzed_documents"绕过覆盖率检查。

### 已排除项

- **#2（gate 覆盖率 45% 却 DONE）**：经核实，那次 qdrant v1.18.2 run（2026-06-11 06:00 UTC）发生在 gate 路径 bug 修复（2026-06-13 04:39 UTC）之前，是历史遗留产物，gate 当前工作正常。不纳入本批次。

---

## 2. 目标与非目标

### 目标

1. 攻击 Agent 从源头获得当前 target 的正确端点/字段信息（契约速查表注入）
2. 三个 attack agent 的示例代码与输出模板不再硬编码任何 DB 特定值
3. safe_request 三 agent 统一为单一权威定义
4. 新增 target-aware 验证器，在 Stage 1 强制拦截"文不对题"脚本
5. 修复 gate 空声明绕过

### 非目标

- 不改变契约生成层（`_target_api_reference.md` 已正确，保持不变）
- 不重写攻击策略本身（策略 1-N 的语义不变，只改示例的写法）
- 不在本批次建立完整测试基础设施（属批次 B），本批次仅自带最小验证
- 不处理 LLM 仍可能生成的边缘硬编码——验证器用启发式签名表覆盖高置信度指纹，不追求 100% 召回

---

## 3. 架构总览

三层防御，数据自上而下：

```
structured_contract.json  (唯一真理源: target + api_endpoints + data_types)
        │
        ├─① 组件 C · reconstruct_context.py  【源头注入】
        │     提取 target + 端点速查表(method/path/category) + data_types
        │     → 注入 attack agent prompt
        │
        ▼
   attack agent  【组件 #1/#3/#1a · 模板层】
     示例代码/输出模板改为契约占位符, safe_request 统一
     生成脚本时引用注入的端点表, 不硬编码任何 DB 值
        │
        ▼
   Stage 1 验证  【组件 B · 强制执行】
     ├ validate_api_format.py         (已有: safe_request 包装检测)
     └ validate_target_neutrality.py  (新增: target-aware 签名检测)
        读 contract.target + 最小签名表 → REJECT 与 target 不符的 DB 语法
        │
        ▼
   执行 → judge → reporter
        │
        ▼
   Stop hook gate  【组件 #2' · 兜底】
     phase=DONE + ATTACK_GEN 已完成 + analyzed_documents 缺失 → 拦截
```

---

## 4. 关键设计决策

### 决策 1：生成层与检测层分离

`_target_api_reference.md` 明确反对"写 per-DB 表"（怕版本变化时过时）。但验证器要检测"target 不符的语法"又需要每个 DB 的语法指纹。解法是关注点分离：

- **生成层**（attack agent 模板 + reconstruct_context 注入）：纯契约驱动，**不读签名表**，保持 `_target_api_reference.md` 原则不变。
- **检测层**（validate_target_neutrality.py）：用一张**最小签名表**（仅含高置信度指纹），文件头明确标注"仅用于检测启发式，不用于生成"。

两层解耦：生成层不背"per-DB 表"的债，检测层有实用工具。

### 决策 2：验证器 target-aware，非一刀切

不能"禁止一切 qdrant 语法"——当 target 真的是 qdrant 时，qdrant 语法正确。验证器比对 `contract.target`：

- `target=qdrant` → qdrant 签名合法，weaviate/pgvector/milvus 签名出现才 REJECT
- `target=weaviate` → 出现 qdrant 端口 `6333` / 路径 `/collections/.../points` → REJECT

只惩罚"文不对题"，不误伤正确 target。

---

## 5. 组件设计

### 组件 C · 契约端点速查表注入

**文件**：`scripts/reconstruct_context.py`

**改造点 1** — `reconstruct()` 第 162-170 行。当前仅统计 `endpoint_count`/`constraint_count`。新增提取速查表：

```python
# 5. structured_contract.json — 速查表提取
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

result["target_reference"] = {
    "target": str(target),
    "endpoint_cheatsheet": endpoint_cheatsheet,
    "key_data_types": contract.get("data_types", []) if contract else [],
}
```

**改造点 2** — `format_text()` 在「### 本轮关键信息」之前插入新 section：

```
### 当前 Target 端点速查表（契约驱动——生成脚本时引用此表，禁止硬编码端口/路径）
- Target: {target}
- 端点数: {N}
| Method | Path | Category |
|--------|------|----------|
| PUT | collections+{collection_name} | collections |
| ... | ... | ... |
- 数据字段命名/向量格式: 见 contract.data_types（key_data_types 已注入）
- ⛔ 禁止写死端口(6333/8080/19530)、路径、payload/properties 字段名——一律从本表或 contract 推导
```

注入机制：reconstruct_context 输出已被 orchestrator 在 ATTACK_GEN 阶段注入 attack agent prompt（既有路径，无需新增调用点）。

### 组件 #1 · 示例代码去 Qdrant 硬编码

**文件**：`attack-boundary.md` / `attack-state.md` / `attack-semantic.md`

把策略示例代码里的 Qdrant 具体值替换为契约引用 + 注释。统一替换规则：

| 硬编码类型 | 原值 | 替换为 |
|-----------|------|--------|
| 端口/URL | `http://localhost:6333` | `BASE_URL`（已 = `os.environ.get("TESTVDB_DB_URL")`） |
| 路径 | `/collections/{name}/points/search` 等 | 注释 `# path 从注入的端点速查表读（当前 target 的实际路径）` + 占位 `safe_request("METHOD", "<cheatsheet path for X>")` |
| 数据字段 | `payload`/`properties`/`vector`/`Class` | 注释 `# 字段命名按 contract.data_types 推导` |
| 过滤语法 | `{"must":[{"key":...,"match":...}]}` | 注释 `# 过滤语法按 contract.target（qdrant=must/match, weaviate=where, milvus=expr）` |
| 响应键 | `body["result"]`/`.get("result")` | 注释 `# 响应键按 contract.target 动态选，先 print(raw_text)，HTTP status 为主判定` |

**精确行号**（已逐行核实）：
- `attack-boundary.md`: 124-127（策略1示例）、157-165（策略3维度示例）、255（输出模板 safe_request 调用）、273（响应键 body.get("status")）
- `attack-state.md`: 112、117-118、121-122（策略1 count）、133（策略2）、166-167（策略4并发）、187（count 响应）、328-330（健壮性示例 .get("result")）
- `attack-semantic.md`: 130、136、141（策略1）、204（策略3）、219、229、237（策略4类型转换）、262、270（策略5搜索）、296-298（策略6 metamorphic）、329-331（策略7 filter must/match）

保留顶部「⛔ 契约驱动」声明（第 15-24 行）——它仍然正确，只是现在下方示例终于与之一致。

### 组件 #3 · analyzed_documents 示例去 weaviate URL 硬编码

**文件**：三个 agent 的「Analyzed Documents 产出契约」section
- `attack-boundary.md`: 329-335
- `attack-state.md`: 357-363
- `attack-semantic.md`: 421-427

**改法**：示例 URL 替换为占位符 + 明确说明：

```markdown
### 输出格式
## Analyzed Documents — boundary
- <逐字复制 raw_knowledge.md Document Sources 表第 1 行的 URL>
- <逐字复制第 2 行的 URL>
- <... 直到覆盖 ≥ 60% 的 Document Sources>

> ⚠️ 上方不提供具体 URL 示例——因为 URL 随 target 变化（qdrant/weaviate/milvus/pgvector 各不同）。
> 照抄任何"看起来像"的 URL 会导致 gate 精确比对失败（覆盖率 0%）。
> 唯一正确做法：Read raw_knowledge.md → 找 ## Document Sources 表 → 逐字复制 URL 列。
```

### 组件 #1a · safe_request 三 agent 统一

**权威定义**（采用 boundary 的三元组版本，最完备）提取到 `agents/_target_api_reference.md` 新增 section：

```python
def safe_request(method, path, **kwargs):
    """Resilient HTTP wrapper. Returns (status_code, body_or_None, raw_text).
    On connection failure: prints REQUEST_ERROR, returns (0, None, "").
    On JSON decode failure: prints JSON_DECODE_ERROR, returns (status, None, text)."""
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

`BASE_URL` 与 `AUTH_HEADER` 为模块级变量，来源沿用 attack-boundary.md 现有约定：`BASE_URL = os.environ.get("TESTVDB_DB_URL")`（无默认端口，缺失则打印 `VERDICT: SCRIPT_ERROR` 退出）、`AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")`。三者（BASE_URL / AUTH_HEADER / safe_request）一并收录进 `_target_api_reference.md` 作为权威定义。

三个 agent 的「输出格式」section 改为引用：「safe_request 权威定义见 `agents/_target_api_reference.md`。所有 HTTP 调用必须用此包装器，返回三元组 `(status, body, raw_text)`。」+ 一个调用示例。删除三份重复定义。

state/semantic 中所有 `status, body = safe_request(...)` 调用点改为 `status, body, raw = safe_request(...)`，判定逻辑用 `raw` 做 `print` 输出。

### 组件 B · target 中立验证器

**新文件**：`scripts/validate_target_neutrality.py`

```python
#!/usr/bin/env python3
"""Target neutrality validator — Stage 1 gate.

DETECTION-ONLY signature table. NOT used for script generation.
See agents/_target_api_reference.md for the contract-driven generation principle.

Reads {session_dir}/structured_contract.json → target, then scans all
generated *.py for DB signatures that DON'T match the current target.
target-aware: qdrant syntax is legal when target=qdrant, REJECT only when
mismatched (e.g. target=weaviate but script hits :6333).
"""
```

**最小签名表**（检测专用）：

```python
SIGNATURES: dict[str, dict[str, list[str]]] = {
    "qdrant": {
        "ports": ["6333"],
        "paths": [r"/collections/[\w-]+/points", r"/collections/\{[^}]+\}/points",
                  r"/collections/[\w-]+/points/search", r"/collections/[\w-]+/points/count"],
        "filter_keys": [r'"must"\s*:', r"'must'\s*:", r'"match"\s*:'],
        "resp_keys": [r'\[\s*"result"\s*\]', r'\.get\(\s*"result"'],
    },
    "weaviate": {
        "ports": ["8080"],
        "paths": [r"/v1/objects", r"/v1/schema", r"/objects", r"/schema"],
        "filter_keys": [r'"where"\s*:', r'"operator"\s*:'],
        "resp_keys": [r'\[\s*"data"\s*\]', r'\.get\(\s*"data"'],
    },
    "milvus": {
        "ports": ["19530"],
        "paths": [r"/v2/vectordb/"],
        "filter_keys": [r'"expr"\s*:'],
        "resp_keys": [],
    },
    "pgvector": {
        "ports": ["5432"],
        "paths": [],          # SQL, no HTTP path fingerprint
        "filter_keys": [],
        "resp_keys": [],
    },
}
```

**判定逻辑**：

1. 读 `{session_dir}/structured_contract.json` → `target`
2. 对每个 `{session_dir}/**/*.py`（boundary_scripts/、state_scripts/、scripts/ 递归，**跳过 `/mre/` 目录**——与 validate_api_format.py:25 一致，reporter-mre 脚本不参与攻击验证）：
   - 检测脚本命中的所有 DB 签名
   - 命中的签名中，若存在 **`target` 以外的 DB** → `REJECT`，列出违规 DB + 具体模式 + 行号
   - 仅命中 `target` 自身签名 → `PASS`
3. 输出 JSON：`{target_neutrality_violations: [{file, foreign_db, evidence: [...]}]}` + 控制台 REJECT/WARN 摘要
4. 退出码：有 REJECT → `1`；全 PASS → `0`

**误报控制**：
- 端口：仅在 URL/连接上下文匹配（`:{port}` 或 `{port}/`），避免匹配无关数字
- 响应键：仅在 dict 访问语法匹配（`["result"]` / `.get("result")`），避免命中变量名/字符串字面量
- pgvector 无 HTTP 指纹，主要靠"出现其他 DB 的端口/路径"反推（pgvector target 下出现任何 HTTP DB 签名 → REJECT）

**集成** — `commands/mine.md` Stage 1（8c）第 5 步 `validate_api_format`（第 408 行）之后插入新步骤，原第 6/7 步顺延：

```
6. **Target 中立验证**：`python scripts/validate_target_neutrality.py "results/{target}/{version}/{timestamp}"`
   含与当前 target 不符的 DB 签名的脚本 → 打回 Attack Agent 修改（同 8d.5 打回机制）。
```

### 组件 #2' · gate 空声明绕过修复

**文件**：`scripts/hooks/pipeline_gate.py`，`check_doc_coverage` 函数第 248-256 行

**现状**（248-256）：
```python
analyzed = _parse_analyzed_docs(round_dir)
if not analyzed:
    log.warning("doc-coverage: no analyzed_documents*.md in %s — cannot verify", round_dir)
    return True, "skipped (no analyzed_documents yet)"
```

**改法**：区分"agent 还没跑"（放行）vs"agent 跑了但没写"（拦截）：

```python
analyzed = _parse_analyzed_docs(round_dir)
if not analyzed:
    attack_ran = (
        str(state.get("phase", "")).upper() == "DONE"
        and "ATTACK_GEN" in state.get("phases_completed", [])
    )
    if attack_ran:
        return False, (
            "Symptom ① — ATTACK_GEN completed but NO analyzed_documents written "
            "(空声明绕过). Attack agents must each emit analyzed_documents_*.md "
            "listing every raw_knowledge.md Document Source URL (see agents/attack-*.md)."
        )
    log.warning("doc-coverage: no analyzed_documents*.md in %s — cannot verify", round_dir)
    return True, "skipped (no analyzed_documents yet — ATTACK_GEN not completed)"
```

不影响既有"no active pipeline"/"anti-loop release"路径。

---

## 6. 测试策略

本批次自带最小验证；完整测试基础设施属批次 B。

| 组件 | 验证方式 | 通过标准 |
|------|---------|---------|
| C | 单元测试：`reconstruct()` 对 qdrant 运行产物提取 `target_reference` | `endpoint_cheatsheet` 非空、长度 = contract 端点数（qdrant 实测 73） |
| #1/#3/#1a | grep 验证三个 agent 示例区 | 示例代码区不含裸 `6333`、`/collections/.../points`、具体 weaviate URL；safe_request 仅在 `_target_api_reference.md` 定义一次 |
| B | 单元测试 `validate_target_neutrality.py` | weaviate-target 脚本含 `:6333` → REJECT；qdrant-target 含 `:6333` → PASS；weaviate-target 含 `/v1/objects` → PASS |
| #2' | 扩展 `_test_pipeline_gate.py` 新增场景 | phase=DONE + phases_completed 含 ATTACK_GEN + analyzed_documents 空 → exit 2 |

**端到端验证**（批次 A 完成后）：跑 `/testvdb:mine weaviate 1.38.0 --max-rounds 1`，确认生成的脚本端口/路径来自 weaviate 契约（8080 + `/objects` 等），而非 Qdrant 的 6333。

---

## 7. 验收标准

1. `reconstruct_context.py --format text` 输出含「当前 Target 端点速查表」section，列出当前 target 的实际端点
2. 三个 attack agent 示例区零硬编码（grep 验证通过）
3. `_target_api_reference.md` 含唯一 safe_request 权威定义；三 agent 引用而非重定义
4. `validate_target_neutrality.py` 存在并通过单测；mine.md Stage 1 第 6 步接入
5. `pipeline_gate.py` 空 analyzed_documents + ATTACK_GEN completed → exit 2（_test_pipeline_gate.py 新场景通过）
6. weaviate 端到端 run 生成的脚本不含 Qdrant 签名

---

## 8. 风险与取舍

| 风险 | 缓解 |
|------|------|
| 签名表过时（DB 版本变路径/端口） | 表仅含高置信度稳定指纹（默认端口、核心路径前缀）；版本化路径变化由契约速查表（组件 C）覆盖，不依赖签名表 |
| 验证器误报（合法脚本被 REJECT） | 端口/响应键加上下文约束（URL 上下文、dict 访问语法）；REJECT 列出具体 evidence 供 Agent 定位；打回机制可迭代 |
| 端点速查表 73 条 token 偏重 | 精简为 method/path/category 三字段（去掉 description/parameters 长文本）；73 行 markdown 可接受，远小于 raw_knowledge |
| LLM 仍生成签名表未覆盖的硬编码 | 启发式不追求 100% 召回；gate（组件 #2'）+ safe_request 统一 + 源头注入（组件 C）多层兜底；未覆盖的由批次 B 测试逐步发现 |

---

## 9. 实现顺序（供 writing-plans 参考）

依赖关系决定顺序：

1. 组件 C（源头注入）— 无依赖，先做，是 #1 示例引用的对象
2. 组件 #1a（safe_request 统一到 _target_api_reference.md）— 无依赖
3. 组件 #1（示例去硬编码）— 依赖 C 的速查表概念 + #1a 的 safe_request 定义
4. 组件 #3（analyzed_documents 示例去硬编码）— 独立，可与 #1 并行
5. 组件 B（验证器）— 依赖签名表设计（本 spec 已定），独立实现
6. 组件 #2'（gate 修复）— 独立

验证：每组件单测 → grep 验证 → weaviate 端到端 run。
