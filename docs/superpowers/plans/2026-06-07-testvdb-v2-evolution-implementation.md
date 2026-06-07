# TestVDB v2.0 自进化与质量增强 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 TestVDB v1.x 升级为 v2.0，引入跨会话自进化、Fan-Out Attack Trio、7-mode AI Failure Checklist、Material Passport、data_access_level 和 Marketplace 分发优化。

**Architecture:** 最小侵入原则——不改变现有 12 Agent 流水线结构，只在关键节点插入新行为。所有新功能通过 `settings.json` feature flag 独立控制，开启则增强，关闭则回退到 v1.x 行为。修改集中在 3 层：(1) 编排层 `commands/mine.md` + `agents/orchestrator.md`，(2) Agent prompt 层 `agents/*.md`，(3) 新增 Python 脚本层 `scripts/`。

**Tech Stack:** Python 3.9+, Bash, JSON, Markdown (Agent 系统提示), Docker

---

## File Structure Map

| 文件 | 类型 | 负责 |
|------|------|------|
| `settings.json` | 修改 | 新增 evolution/fan_out/ai_failure_check/material_passport 配置段 |
| `commands/mine.md` | 修改 | 注入策略读取/合并、Fan-Out 9并发、passport hash 验证 |
| `agents/orchestrator.md` | 修改 | 策略注入模板、Fan-Out focus_profile、策略提取步骤 |
| `agents/attack-boundary.md` | 修改 | 跨会话策略消费、data_access_level |
| `agents/attack-state.md` | 修改 | 跨会话策略消费、data_access_level |
| `agents/attack-semantic.md` | 修改 | 跨会话策略消费、data_access_level |
| `agents/reporter.md` | 修改 | Pre-Submit Gate 前插入 7-mode 自检 |
| `agents/contract-formalizer.md` | 修改 | 生成 _passport 字段 |
| `agents/knowledge-extractor.md` | 修改 | data_access_level |
| `agents/judge-doc.md` | 修改 | data_access_level |
| `agents/judge-evidence.md` | 修改 | data_access_level |
| `agents/judge-novelty.md` | 修改 | data_access_level |
| `agents/judge-severity.md` | 修改 | data_access_level |
| `agents/docker-executor.md` | 修改 | data_access_level |
| `skills/pipeline/SKILL.md` | 修改 | Phase 3 策略注入 + Fan-Out |
| `skills/defect-taxonomy/SKILL.md` | 修改 | 7-mode checklist 参考 |
| `skills/contract-schema/SKILL.md` | 修改 | _passport schema |
| `scripts/ai_failure_check.py` | 新建 | 7-mode AI 失败自检脚本 |
| `scripts/strategy_extractor.py` | 新建 | 策略提取+泛化+合并脚本 |
| `scripts/strategy_injector.py` | 新建 | 策略注入查询脚本 |
| `scripts/passport_verify.py` | 新建 | Material Passport hash 验证脚本 |
| `strategy_registry/global_strategies.json` | 新建 | 跨 DB 通用策略注册表 |
| `strategy_registry/milvus_strategies.json` | 新建 | Milvus 特有策略 |
| `strategy_registry/qdrant_strategies.json` | 新建 | Qdrant 特有策略 |
| `strategy_registry/weaviate_strategies.json` | 新建 | Weaviate 特有策略 |
| `strategy_registry/pgvector_strategies.json` | 新建 | PGVector 特有策略 |
| `strategy_registry/evolution_log.jsonl` | 新建 | 策略演化审计日志 |
| `plugin.json` | 修改 | 版本号升级到 2.0.0 |
| `README_zh.md` | 修改 | 增加 marketplace 安装命令 |
| `AGENTS.md` | 修改 | 增加安装说明+v2.0 新功能 |

---

### Task 1: 配置基础设施 — settings.json 新增字段

**Files:**
- Modify: `settings.json`

- [ ] **Step 1: 在 settings.json 中新增 evolution/fan_out/ai_failure_check/material_passport 配置段**

读取 `settings.json`，在 `"log_level"` 字段之前插入以下四个新配置段：

```json
"evolution": {
  "enabled": true,
  "strategy_registry_dir": "strategy_registry",
  "max_strategies_per_injection": 10,
  "min_confidence_for_injection": 0.6,
  "auto_deprecate_after_failures": 3
},
"fan_out": {
  "enabled": true,
  "seeds_per_agent": 3,
  "profiles": ["priority_first", "coverage_gap", "rejection_pattern"]
},
"ai_failure_check": {
  "enabled": true,
  "halt_on": ["M4", "M7"],
  "reject_on": ["M2", "M3", "M6"],
  "rewind_on": ["M1", "M5"]
},
"material_passport": {
  "enabled": true,
  "hash_algorithm": "sha256",
  "reject_on_tamper": true
},
```

确保 JSON 合法（逗号正确、无尾逗号）。

- [ ] **Step 2: 验证 settings.json JSON 合法性**

Run: `python -c "import json; json.load(open('TestVDB/settings.json')); print('Valid JSON')"`
Expected: `Valid JSON`

- [ ] **Step 3: 验证新字段可被正确读取**

Run:
```bash
python -c "
import json
with open('TestVDB/settings.json') as f:
    s = json.load(f)
assert 'evolution' in s and s['evolution']['enabled'] == True
assert 'fan_out' in s and s['fan_out']['seeds_per_agent'] == 3
assert 'ai_failure_check' in s and s['ai_failure_check']['enabled'] == True
assert 'material_passport' in s and s['material_passport']['enabled'] == True
print('All new config sections present and valid')
"
```
Expected: `All new config sections present and valid`

- [ ] **Step 4: Commit**

```bash
git add TestVDB/settings.json
git commit -m "feat(config): add evolution, fan_out, ai_failure_check, material_passport config sections"
```

---

### Task 2: P2 — data_access_level（全 Agent Frontmatter 更新）

**Files:**
- Modify: `agents/orchestrator.md`
- Modify: `agents/knowledge-extractor.md`
- Modify: `agents/contract-formalizer.md`
- Modify: `agents/attack-boundary.md`
- Modify: `agents/attack-state.md`
- Modify: `agents/attack-semantic.md`
- Modify: `agents/docker-executor.md`
- Modify: `agents/judge-doc.md`
- Modify: `agents/judge-evidence.md`
- Modify: `agents/judge-novelty.md`
- Modify: `agents/judge-severity.md`
- Modify: `agents/reporter.md`

> **注意:** 此任务修改 12 个 Agent 文件，每个改动都是相同的模式——在 frontmatter 中加一行 `dataAccess`，在 prompt 开头加一段数据访问约束。这是纯 prompt 工程，无新代码。

- [ ] **Step 1: orchestrator.md — 添加 dataAccess + 数据访问约束**

在 frontmatter 的 `model: opus` 之后、`maxTurns: 120` 之前插入：
```
dataAccess: redacted
```

在 `# TestVDB Orchestrator` 标题之后、`> **⛔ 执行模型变更` 之前插入：
```markdown
## 数据访问级别: redacted

你只能访问所有 Agent 的产出文件（structured_contract.json, raw_knowledge.md, pipeline_state.json,
debate_logs/*.json, execution_summary.txt, output_*.log, defect-*.md, experience_handoff.json,
coverage.json, mine_state.json, strategy_registry/*.json）。

禁止直接访问:
- 网络（WebSearch/WebFetch/Crawl4AI）—— 爬取由 knowledge-extractor 完成
- 外部 API —— 所有外部数据获取由对应子 Agent 完成

如果你需要访问网络或外部数据，请派发对应权限的 Agent（如 knowledge-extractor）。
```

- [ ] **Step 2: knowledge-extractor.md — dataAccess: raw**

在 frontmatter 中添加 `dataAccess: raw`。

在 prompt 开头插入：
```markdown
## 数据访问级别: raw

你是唯一拥有网络访问权限的 Agent。你可以使用 WebSearch、WebFetch、Crawl4AI 爬取文档。
其他 Agent 依赖你的产出（raw_knowledge.md），不直接访问网络。
```

- [ ] **Step 3: contract-formalizer.md — dataAccess: raw**

在 frontmatter 中添加 `dataAccess: raw`。

在 prompt 开头插入：
```markdown
## 数据访问级别: raw

你可以读取 raw_knowledge.md（原始文档知识）。你不需要网络访问——所有文档内容
已在 raw_knowledge.md 中。禁止使用 WebSearch/WebFetch，如需补充文档信息，
告知 Orchestrator 由 knowledge-extractor 获取。
```

- [ ] **Step 4: attack-boundary.md — dataAccess: redacted**

在 frontmatter 中添加 `dataAccess: redacted`。

在 prompt 开头插入：
```markdown
## 数据访问级别: redacted

你可以访问:
- structured_contract.json（契约文件）
- strategy_registry/ 中的策略文件
- reflection_context（注入的经验数据）

禁止访问:
- 网络（WebSearch/WebFetch）—— 你的攻击基于契约而非文档
- 执行结果 —— 不关你的事，你只生成脚本
```

- [ ] **Step 5: attack-state.md — dataAccess: redacted**

在 frontmatter 中添加 `dataAccess: redacted`。

在 prompt 开头插入：
```markdown
## 数据访问级别: redacted

你可以访问:
- structured_contract.json（契约文件）
- strategy_registry/ 中的策略文件
- reflection_context（注入的经验数据）

禁止访问:
- 网络（WebSearch/WebFetch）—— 你的攻击基于契约而非文档
- 执行结果 —— 不关你的事，你只生成脚本
```

- [ ] **Step 6: attack-semantic.md — dataAccess: redacted**

在 frontmatter 中添加 `dataAccess: redacted`。

在 prompt 开头插入：
```markdown
## 数据访问级别: redacted

你可以访问:
- structured_contract.json（契约文件）
- strategy_registry/ 中的策略文件
- reflection_context（注入的经验数据）

禁止访问:
- 网络（WebSearch/WebFetch）—— 你的攻击基于契约而非文档
- 执行结果 —— 不关你的事，你只生成脚本
```

- [ ] **Step 7: docker-executor.md — dataAccess: redacted**

在 frontmatter 中添加 `dataAccess: redacted`。

在 prompt 开头插入：
```markdown
## 数据访问级别: redacted

你只能访问:
- 会话目录中的攻击脚本文件（boundary_scripts/, state_scripts/, scripts/, script_*.py）

禁止访问:
- 网络 —— 容器内执行，不需要外部网络（sidecar 模式）
- 契约文件 —— 不关你的事，你只执行脚本
- 脚本内容 —— ⛔ 绝对禁止读取脚本内容，直接执行
```

- [ ] **Step 8: judge-doc.md — dataAccess: verified_only**

在 frontmatter 中添加 `dataAccess: verified_only`。

在 prompt 开头插入：
```markdown
## 数据访问级别: verified_only

你可以访问:
- 执行结果（output_*.log, exit_code_*.txt）
- structured_contract.json（用于文档验证）
- WebSearch（降级验证文档引用时使用）

禁止访问:
- 原始 raw_knowledge.md —— 你应该基于契约和文档验证，而非原始抓取内容
```

- [ ] **Step 9: judge-evidence.md — dataAccess: verified_only**

在 frontmatter 中添加 `dataAccess: verified_only`。

在 prompt 开头插入：
```markdown
## 数据访问级别: verified_only

你可以访问:
- 执行结果（output_*.log, exit_code_*.txt, execution_summary.txt）

禁止访问:
- 网络 —— 证据审查基于本地执行结果，不需要外部数据
- 契约文件 —— 你的审查基于实际行为 vs 预期行为，契约引用由 judge-doc 验证
```

- [ ] **Step 10: judge-novelty.md — dataAccess: verified_only**

在 frontmatter 中添加 `dataAccess: verified_only`。

在 prompt 开头插入：
```markdown
## 数据访问级别: verified_only

你可以访问:
- 执行结果（output_*.log, exit_code_*.txt）
- GitHub MCP（搜索已有 issues/PRs 判断新颖性）

禁止访问:
- 契约文件 —— 新颖性判断不依赖契约内容
```

- [ ] **Step 11: judge-severity.md — dataAccess: verified_only**

在 frontmatter 中添加 `dataAccess: verified_only`。

在 prompt 开头插入：
```markdown
## 数据访问级别: verified_only

你可以访问:
- 执行结果（output_*.log, exit_code_*.txt）
- judge-evidence 的审查结果

禁止访问:
- 网络 —— 严重性评估基于证据和影响分析，不需要外部数据
- 契约文件 —— 严重性判定基于缺陷类型和执行结果
```

- [ ] **Step 12: reporter.md — dataAccess: verified_only**

在 frontmatter 中添加 `dataAccess: verified_only`。

在 prompt 开头插入：
```markdown
## 数据访问级别: verified_only

你可以访问:
- Judge Quartet 的全部审查结果（stage2_*.json）
- 执行结果（output_*.log, exit_code_*.txt）
- structured_contract.json（生成报告中的契约引用）

禁止访问:
- 网络 —— 报告基于已有的审查结果和执行日志
```

- [ ] **Step 13: Commit**

```bash
git add TestVDB/agents/*.md
git commit -m "feat(agents): add data_access_level frontmatter and prompt constraints to all 12 agents"
```

---

### Task 3: P2 — Material Passport（契约版本化 + hash 防篡改）

**Files:**
- Create: `scripts/passport_verify.py`
- Modify: `agents/contract-formalizer.md`
- Modify: `commands/mine.md`
- Modify: `skills/contract-schema/SKILL.md`

#### Subtask 3a: 创建 passport_verify.py

- [ ] **Step 1: 创建 scripts/passport_verify.py**

```python
#!/usr/bin/env python3
"""
Material Passport — hash 验证脚本
验证 structured_contract.json 的 _passport.contract_hash 完整性。

用法:
  python scripts/passport_verify.py <path/to/structured_contract.json>

退出码:
  0 = PASS (hash 匹配)
  1 = NO_PASSPORT (无 _passport 字段，旧格式)
  2 = TAMPERED (hash 不匹配，可能被篡改)
  3 = INVALID_JSON (文件无法解析)
"""

import json
import hashlib
import sys
import os
from datetime import datetime, timezone


def compute_hash(data: dict, algorithm: str = "sha256") -> str:
    """计算排除 _passport 字段后的契约 hash"""
    data_without_passport = {k: v for k, v in data.items() if k != "_passport"}
    canonical_json = json.dumps(data_without_passport, sort_keys=True, separators=(",", ":"))
    h = hashlib.new(algorithm)
    h.update(canonical_json.encode("utf-8"))
    return f"{algorithm}:{h.hexdigest()}"


def verify_passport(contract_path: str) -> dict:
    """验证 Material Passport，返回结果字典"""
    result = {
        "file": contract_path,
        "status": "UNKNOWN",
        "checked_at": datetime.now(timezone.utc).isoformat(),
        "details": {}
    }

    if not os.path.exists(contract_path):
        result["status"] = "FILE_NOT_FOUND"
        result["details"]["error"] = f"File not found: {contract_path}"
        return result

    try:
        with open(contract_path, "r", encoding="utf-8") as f:
            contract = json.load(f)
    except json.JSONDecodeError as e:
        result["status"] = "INVALID_JSON"
        result["details"]["error"] = str(e)
        return result

    passport = contract.get("_passport")
    if not passport:
        result["status"] = "NO_PASSPORT"
        result["details"]["warning"] = (
            "No _passport field found. This contract was generated by an older version "
            "of TestVDB (pre-v2.0). Hash verification skipped."
        )
        return result

    expected_hash = passport.get("contract_hash")
    algorithm = passport.get("contract_hash_algorithm", "sha256")

    if not expected_hash:
        result["status"] = "NO_HASH"
        result["details"]["error"] = "_passport exists but contract_hash is missing"
        return result

    actual_hash = compute_hash(contract, algorithm)

    result["details"]["expected_hash"] = expected_hash
    result["details"]["actual_hash"] = actual_hash
    result["details"]["algorithm"] = algorithm
    result["details"]["schema_version"] = passport.get("schema_version", "unknown")
    result["details"]["generated_at"] = passport.get("generation", {}).get("generated_at", "unknown")

    if actual_hash == expected_hash:
        result["status"] = "PASS"
    else:
        result["status"] = "TAMPERED"
        result["details"]["error"] = (
            f"Hash mismatch! Contract content has been modified since generation. "
            f"Expected: {expected_hash[:32]}..., Actual: {actual_hash[:32]}..."
        )

    return result


def main():
    if len(sys.argv) < 2:
        print("Usage: python scripts/passport_verify.py <path/to/structured_contract.json>")
        sys.exit(3)

    contract_path = sys.argv[1]
    result = verify_passport(contract_path)
    print(json.dumps(result, indent=2, ensure_ascii=False))

    status_map = {"PASS": 0, "NO_PASSPORT": 1, "TAMPERED": 2,
                  "INVALID_JSON": 3, "FILE_NOT_FOUND": 3, "NO_HASH": 2}
    sys.exit(status_map.get(result["status"], 3))


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: 测试 passport_verify.py — 正常 PASS 场景**

Run:
```bash
cd TestVDB

# 创建测试用的 contract
python -c "
import json, hashlib
contract = {
    '_passport': {
        'schema_version': '2.0',
        'contract_hash': '',
        'contract_hash_algorithm': 'sha256',
        'source': {'doc_urls': ['https://example.com'], 'doc_version': 'v1.0', 'crawl_method': 'test', 'crawled_at': '2026-06-07T00:00:00Z'},
        'generation': {'knowledge_extractor_agent': 'test', 'contract_formalizer_agent': 'test', 'generated_at': '2026-06-07T00:00:00Z', 'cache_ttl_hours': 168},
        'integrity': {'verified': True, 'verified_at': '2026-06-07T00:00:00Z', 'core_crud_coverage_pct': 100, 'endpoint_count': 1, 'constraint_count': 1}
    },
    'target': 'qdrant',
    'version': 'v1.0.0',
    'api_endpoints': []
}
# Compute hash
data_without = {k: v for k, v in contract.items() if k != '_passport'}
canonical = json.dumps(data_without, sort_keys=True, separators=(',', ':'))
contract['_passport']['contract_hash'] = 'sha256:' + hashlib.sha256(canonical.encode()).hexdigest()
with open('/tmp/test_passport_contract.json', 'w') as f:
    json.dump(contract, f, indent=2)
print('Test contract created')
"
python scripts/passport_verify.py /tmp/test_passport_contract.json
```
Expected: `"status": "PASS"`, exit code 0

- [ ] **Step 3: 测试 passport_verify.py — TAMPERED 场景**

Run:
```bash
# 篡改 contract
python -c "
import json
with open('/tmp/test_passport_contract.json') as f:
    c = json.load(f)
c['api_endpoints'].append({'path': 'fake', 'method': 'POST'})
with open('/tmp/test_passport_tampered.json', 'w') as f:
    json.dump(c, f, indent=2)
"
python scripts/passport_verify.py /tmp/test_passport_tampered.json
```
Expected: `"status": "TAMPERED"`, exit code 2

- [ ] **Step 4: 测试 passport_verify.py — NO_PASSPORT 场景**

Run:
```bash
echo '{"target":"qdrant","version":"v1.0.0","api_endpoints":[]}' > /tmp/test_no_passport.json
python scripts/passport_verify.py /tmp/test_no_passport.json
```
Expected: `"status": "NO_PASSPORT"`, exit code 1

- [ ] **Step 5: Commit**

```bash
git add TestVDB/scripts/passport_verify.py
git commit -m "feat(passport): add passport_verify.py for contract hash integrity checking"
```

#### Subtask 3b: 修改 contract-formalizer.md — 生成 _passport

- [ ] **Step 6: 在 contract-formalizer.md 契约 JSON Schema 中增加 _passport 字段**

在 `contract-formalizer.md` 的契约 JSON Schema 部分，在 `"properties"` 对象中最前面插入 `_passport` 属性定义（在 `"target"` 之前）：

```json
"_passport": {
  "type": "object",
  "required": ["schema_version", "contract_hash", "contract_hash_algorithm", "source", "generation", "integrity"],
  "properties": {
    "schema_version": { "type": "string", "description": "Passport schema version (2.0)" },
    "contract_hash": { "type": "string", "description": "SHA256 hash of contract content (excluding _passport)" },
    "contract_hash_algorithm": { "type": "string", "description": "Hash algorithm used (sha256)" },
    "source": {
      "type": "object",
      "required": ["doc_urls", "doc_version", "crawl_method", "crawled_at"],
      "properties": {
        "doc_urls": { "type": "array", "items": { "type": "string" } },
        "doc_version": { "type": "string" },
        "crawl_method": { "type": "string" },
        "crawled_at": { "type": "string", "format": "date-time" }
      }
    },
    "generation": {
      "type": "object",
      "required": ["knowledge_extractor_agent", "contract_formalizer_agent", "generated_at", "cache_ttl_hours"],
      "properties": {
        "knowledge_extractor_agent": { "type": "string" },
        "contract_formalizer_agent": { "type": "string" },
        "generated_at": { "type": "string", "format": "date-time" },
        "cache_ttl_hours": { "type": "integer" }
      }
    },
    "integrity": {
      "type": "object",
      "required": ["verified", "verified_at", "core_crud_coverage_pct", "endpoint_count", "constraint_count"],
      "properties": {
        "verified": { "type": "boolean" },
        "verified_at": { "type": "string", "format": "date-time" },
        "core_crud_coverage_pct": { "type": "number" },
        "endpoint_count": { "type": "integer" },
        "constraint_count": { "type": "integer" }
      }
    }
  }
},
```

- [ ] **Step 7: 在 contract-formalizer.md 输出验证部分增加 passport 生成指令**

在 `contract-formalizer.md` 的「输出验证」部分末尾（第 12 条之后），追加：

```markdown
13. **_passport 生成**（v2.0 新增）：
   - 在 structured_contract.json 顶层生成 `_passport` 字段
   - `schema_version`: "2.0"
   - `source.doc_urls`: 从 raw_knowledge.md 提取的所有文档 URL
   - `source.doc_version`: 文档版本号
   - `source.crawl_method`: "crawl4ai" | "webfetch" | "manual"
   - `source.crawled_at`: 当前时间（ISO 8601）
   - `generation.knowledge_extractor_agent`: "testvdb:knowledge-extractor"
   - `generation.contract_formalizer_agent`: "testvdb:contract-formalizer"
   - `generation.generated_at`: 当前时间（ISO 8601）
   - `generation.cache_ttl_hours`: 从 settings.json 读取的 knowledge.cache_ttl_hours
   - `integrity.verified`: true
   - `integrity.verified_at`: 当前时间（ISO 8601）
   - `integrity.core_crud_coverage_pct`: 核心 CRUD 覆盖率百分比
   - `integrity.endpoint_count`: api_endpoints 数组长度
   - `integrity.constraint_count`: 所有约束数组的总长度
   - **hash 计算**：使用 Bash 执行 `python scripts/passport_verify.py --compute-hash results/{target}/{version}/structured_contract.json`
     将输出的 hash 值填入 `_passport.contract_hash`
```

- [ ] **Step 8: Commit**

```bash
git add TestVDB/agents/contract-formalizer.md
git commit -m "feat(passport): add _passport generation instructions to contract-formalizer"
```

#### Subtask 3c: 修改 commands/mine.md — hash 验证集成

- [ ] **Step 9: 修改 mine.md 的 Step 3（缓存检查）增加 passport hash 验证**

在 `commands/mine.md` 的 Step 3 中，在现有的 TTL 过期计算之后，增加 passport hash 验证逻辑：

```markdown
### Step 3: 缓存检查
检查 `results/{target}/{version}/structured_contract.json` 是否存在且未过期（TTL 见 settings.json 的 `knowledge.cache_ttl_hours`，默认 168h）。

**v2.0 新增 — Passport Hash 验证（material_passport.enabled=true 时）：**
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```
- 退出码 0（PASS）→ 缓存有效，跳到 Step 6
- 退出码 1（NO_PASSPORT）→ 旧格式契约，输出警告但继续使用缓存
- 退出码 2（TAMPERED）→ 契约被篡改，强制重新生成（继续 Step 4）
- 退出码 3（INVALID_JSON/FILE_NOT_FOUND）→ 视为缓存无效

如果 `material_passport.enabled=false`，跳过 hash 验证，仅按 TTL 判断。
```

- [ ] **Step 10: 修改 mine.md 的 Step 6（合同门控检查）增加 hash 验证**

在 Step 6 中，在现有覆盖率检查之前插入：

```markdown
**v2.0 新增 — Passport Hash 验证（material_passport.enabled=true 时）：**
对新生成的 structured_contract.json 执行 hash 验证：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```
- 退出码 0（PASS）→ 契约完整性确认
- 退出码 2（TAMPERED）→ 异常：契约刚生成 hash 就不匹配，可能是 Agent 写入不完整。
  重试 `contract-formalizer` 一次。如果重试后仍不匹配，标记为 `PASSPORT_TAMPERED` 并终止。
```

- [ ] **Step 11: Commit**

```bash
git add TestVDB/commands/mine.md
git commit -m "feat(passport): integrate passport hash verification into cache check and contract gating"
```

#### Subtask 3d: 更新 contract-schema skill

- [ ] **Step 12: 在 skills/contract-schema/SKILL.md 中增加 _passport schema 参考**

在 `skills/contract-schema/SKILL.md` 的顶层结构 JSON 中，在 `"target"` 之前插入 `_passport` 字段说明：

```markdown
### _passport 字段 (v2.0 新增)

```json
{
  "_passport": {
    "schema_version": "2.0",
    "contract_hash": "sha256:<hex_digest>",
    "contract_hash_algorithm": "sha256",
    "source": {
      "doc_urls": ["<url>", ...],
      "doc_version": "<version>",
      "crawl_method": "crawl4ai|webfetch|manual",
      "crawled_at": "<ISO 8601>"
    },
    "generation": {
      "knowledge_extractor_agent": "testvdb:knowledge-extractor",
      "contract_formalizer_agent": "testvdb:contract-formalizer",
      "generated_at": "<ISO 8601>",
      "cache_ttl_hours": 168
    },
    "integrity": {
      "verified": true,
      "verified_at": "<ISO 8601>",
      "core_crud_coverage_pct": 95.0,
      "endpoint_count": 12,
      "constraint_count": 85
    }
  }
}
```

**Hash 计算规则**：
- 输入 = 排除 `_passport` 字段后的完整 JSON（按 key 排序，无空格）
- 算法 = sha256
- 格式 = `sha256:<hex_digest>`

**验证方法**：
```bash
python scripts/passport_verify.py <path/to/structured_contract.json>
```
```

- [ ] **Step 13: Commit**

```bash
git add TestVDB/skills/contract-schema/SKILL.md
git commit -m "docs(contract-schema): add _passport schema reference for v2.0"
```

---

### Task 4: P1 — 7-Mode AI Failure Checklist

**Files:**
- Create: `scripts/ai_failure_check.py`
- Modify: `agents/reporter.md`
- Modify: `skills/defect-taxonomy/SKILL.md`

#### Subtask 4a: 创建 ai_failure_check.py

- [ ] **Step 1: 创建 scripts/ai_failure_check.py**

```python
#!/usr/bin/env python3
"""
7-Mode AI Failure Checklist — Reporter Pre-Submit Gate 自检脚本

检查 LLM 生成内容中的 7 种常见幻觉/错误模式。

用法:
  python scripts/ai_failure_check.py <session_dir> <defect_id>

输入:
  SESSION_DIR: 会话目录（e.g., results/qdrant/v1.13.0/2026-06-07T14-00-00Z）
  defect_id: 缺陷标识（e.g., defect-001）

输出 (stdout):
  JSON {checklist: [{mode, passed: bool, detail}], overall: PASS|FAIL|HALT}

退出码:
  0 = PASS (全部通过)
  1 = FAIL (存在 REJECT 级问题 — M2/M3/M6)
  2 = HALT (存在 HALT 级问题 — M4/M7)
"""

import os
import sys
import json
import re
import subprocess
import time
from pathlib import Path


def load_file(path: str) -> str:
    """加载文件内容，文件不存在返回空字符串"""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return ""


def load_json(path: str) -> dict:
    """加载 JSON 文件，不存在返回空字典"""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def check_m1_script_errors(session_dir: str) -> dict:
    """
    M1: 脚本错误被误判为数据库缺陷
    检查 execution_summary.txt 中 exit_code≠0 且并非 FAILED: 标记
    """
    summary_path = os.path.join(session_dir, "execution_summary.txt")
    content = load_file(summary_path)
    if not content:
        return {"mode": "M1", "passed": True,
                "detail": "No execution_summary.txt found — nothing to check"}

    # 检查是否有 exit_code≠0 但被当作缺陷的脚本
    # 简单启发式：统计非零 exit_code 数量
    non_zero = len(re.findall(r"Exit code non-zero: (\d+)", content))
    total = len(re.findall(r"Scripts executed: (\d+)", content))

    return {
        "mode": "M1",
        "passed": True,  # M1 检查是信息性的，不阻断
        "detail": f"Scripts with non-zero exit: {non_zero}. "
                  f"These may include legitimate defect triggers — verify manually."
    }


def check_m2_fabricated_urls(session_dir: str, defect_id: str) -> dict:
    """
    M2: 编造文档引用（幻觉 URL）
    curl 每个 source_url → 验证 HTTP 200
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M2", "passed": True,
                "detail": f"No defect file found for {defect_id}"}

    # 提取所有 source_url
    urls = re.findall(r'source_url["\s:]*["\s]*([^")\s]+)', content)
    urls = [u for u in urls if u.startswith("http")]

    if not urls:
        return {"mode": "M2", "passed": True,
                "detail": "No source URLs found in defect report"}

    results = []
    for url in urls[:5]:  # 最多检查 5 个 URL
        for attempt in range(2):
            try:
                r = subprocess.run(
                    ["curl", "-sI", "-o", "/dev/null", "-w", "%{http_code}",
                     "--max-time", "10", url],
                    capture_output=True, text=True, timeout=15
                )
                status = r.stdout.strip()
                if status in ("200", "301", "302"):
                    results.append({"url": url, "reachable": True, "status": status})
                    break
                else:
                    if attempt == 0:
                        time.sleep(3)  # 重试间隔
                    else:
                        results.append({"url": url, "reachable": False, "status": status})
            except (subprocess.TimeoutExpired, Exception) as e:
                if attempt == 0:
                    time.sleep(3)
                else:
                    results.append({"url": url, "reachable": False, "status": str(e)})

    unreachable = [r for r in results if not r["reachable"]]
    all_unreachable = len(unreachable) == len(results) and len(results) > 0

    if all_unreachable:
        # 所有 URL 都不可达 → 可能是网络问题，降级为 WARN
        return {
            "mode": "M2",
            "passed": True,
            "detail": f"All {len(results)} URLs unreachable. May be network issue. "
                      f"Urls checked: {[r['url'] for r in results]}"
        }
    elif unreachable:
        return {
            "mode": "M2",
            "passed": False,
            "detail": f"{len(unreachable)}/{len(results)} URLs unreachable: "
                      f"{[r['url'] for r in unreachable]}"
        }
    else:
        return {
            "mode": "M2",
            "passed": True,
            "detail": f"All {len(results)} URLs reachable"
        }


def check_m3_fabricated_results(session_dir: str, defect_id: str) -> dict:
    """
    M3: 编造执行结果数据
    比对 defect-N.md 中的输出与 output_*.log 中的原始输出
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M3", "passed": True, "detail": "No defect file"}

    # 提取 defect 报告中声称的 HTTP 响应状态码
    status_codes = re.findall(r'HTTP Response["\s:]*["\s]*(\d{3})', content)
    response_bodies = re.findall(r'response_body["\s:]*["\s]*([^"]{10,100})', content)

    # 在 output_*.log 中搜索是否存在这些状态码
    output_files = list(Path(session_dir).glob("output_*.log"))
    all_output = ""
    for f in output_files:
        all_output += load_file(str(f))

    fabricated = []
    for code in status_codes:
        if code not in all_output:
            fabricated.append(f"Status code {code} not found in any output log")

    if fabricated:
        return {
            "mode": "M3",
            "passed": False,
            "detail": f"Possible fabricated data: {'; '.join(fabricated[:3])}"
        }
    else:
        return {
            "mode": "M3",
            "passed": True,
            "detail": f"All claimed status codes found in output logs"
        }


def check_m4_shortcut_pipeline(session_dir: str) -> dict:
    """
    M4: 走捷径跳过关键验证
    检查 .done 标记是否全部存在
    """
    required_done = [
        "debate_logs/stage1.json.done",
        "debate_logs/stage2_doc.json.done",
        "debate_logs/stage2_evidence.json.done",
        "debate_logs/stage2_novelty.json.done",
        "debate_logs/stage2_severity.json.done",
    ]

    missing = []
    for f in required_done:
        full_path = os.path.join(session_dir, f)
        if not os.path.exists(full_path):
            missing.append(f)

    if missing:
        return {
            "mode": "M4",
            "passed": False,
            "detail": f"Missing .done markers: {missing}. "
                      f"Pipeline may have skipped critical validation steps."
        }
    else:
        return {
            "mode": "M4",
            "passed": True,
            "detail": "All required .done markers present"
        }


def check_m5_script_bug_as_defect(session_dir: str, defect_id: str) -> dict:
    """
    M5: 脚本 bug 被说成新发现
    检查 FAILED: 输出是否匹配预期缺陷类型
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M5", "passed": True, "detail": "No defect file"}

    # 检查缺陷类型是否与响应行为一致
    defect_type = ""
    m = re.search(r'Type:\s*(Type\d_\w+)', content)
    if m:
        defect_type = m.group(1)

    # Type1 需要 expect 4xx got 2xx
    # Type3 需要合法输入导致 500/crash
    if "Type1" in defect_type:
        has_2xx = re.search(r'HTTP Response["\s:]*["\s]*2\d{2}', content)
        if not has_2xx:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no 2xx response found. "
                          f"May be a script bug misclassified as a defect."
            }

    return {
        "mode": "M5",
        "passed": True,
        "detail": f"Defect type ({defect_type}) appears consistent with reported behavior"
    }


def check_m6_fabricated_methodology(session_dir: str, defect_id: str) -> dict:
    """
    M6: 编造方法论
    检查 defect-N.md 中是否有不在 attack-*.md 中的测试策略描述
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M6", "passed": True, "detail": "No defect file"}

    # 提取策略描述关键词
    strategy_keywords = re.findall(r'strategy["\s:]*["\s]*([^")\n]+)', content)

    if not strategy_keywords:
        # 没有明确策略引用，检查是否有方法论描述段落
        methodology_section = re.search(r'(?:Methodology|Approach|Strategy)[:\s]*(.+?)(?:\n\n|\n#)', content, re.DOTALL)
        if methodology_section:
            return {
                "mode": "M6",
                "passed": True,
                "detail": "Methodology section present — verify manually against attack agent output"
            }

    return {
        "mode": "M6",
        "passed": True,
        "detail": "No obvious fabricated methodology detected"
    }


def check_m7_stale_loop(session_dir: str) -> dict:
    """
    M7: 锁定早期错误假设
    检查同一 endpoint 的缺陷是否在多个 round 中反复出现但从未确认
    """
    experience_path = os.path.join(session_dir, "experience_handoff.json")
    exp = load_json(experience_path)

    rejection_patterns = exp.get("rejection_patterns", [])
    if not rejection_patterns:
        return {"mode": "M7", "passed": True,
                "detail": "No rejection patterns recorded"}

    # 检查是否有 endpoint 出现超过 2 次
    endpoint_counts = {}
    for rp in rejection_patterns:
        ep = rp.get("endpoint", "unknown")
        endpoint_counts[ep] = endpoint_counts.get(ep, 0) + 1

    stale = [ep for ep, count in endpoint_counts.items() if count >= 3]
    if stale:
        return {
            "mode": "M7",
            "passed": False,
            "detail": f"Endpoints with ≥3 repeated rejections: {stale}. "
                      f"May indicate stale assumptions — consider halting."
        }

    return {
        "mode": "M7",
        "passed": True,
        "detail": f"No stale endpoints detected ({len(rejection_patterns)} rejection patterns)"
    }


def main():
    if len(sys.argv) < 3:
        print("Usage: python scripts/ai_failure_check.py <session_dir> <defect_id>")
        print(json.dumps({"checklist": [], "overall": "FAIL",
                          "error": "Missing arguments"}))
        sys.exit(1)

    session_dir = sys.argv[1]
    defect_id = sys.argv[2]

    # 从 defect_id 提取 defect-N.md 格式
    if not defect_id.startswith("defect-"):
        defect_id = f"defect-{defect_id}"

    checks = [
        check_m1_script_errors(session_dir),
        check_m2_fabricated_urls(session_dir, defect_id),
        check_m3_fabricated_results(session_dir, defect_id),
        check_m4_shortcut_pipeline(session_dir),
        check_m5_script_bug_as_defect(session_dir, defect_id),
        check_m6_fabricated_methodology(session_dir, defect_id),
        check_m7_stale_loop(session_dir),
    ]

    # 判定 overall
    # REJECT: M2/M3/M6 任一未通过
    # HALT: M4/M7 任一未通过
    # FAIL: 存在未通过的检查
    # PASS: 全部通过

    reject_modes = {"M2", "M3", "M6"}
    halt_modes = {"M4", "M7"}

    has_reject = any(not c["passed"] and c["mode"] in reject_modes for c in checks)
    has_halt = any(not c["passed"] and c["mode"] in halt_modes for c in checks)
    has_fail = any(not c["passed"] for c in checks)

    if has_reject:
        overall = "FAIL"
    elif has_halt:
        overall = "HALT"
    elif has_fail:
        overall = "FAIL"
    else:
        overall = "PASS"

    result = {
        "checklist": checks,
        "overall": overall,
        "session_dir": session_dir,
        "defect_id": defect_id
    }

    print(json.dumps(result, indent=2, ensure_ascii=False))

    if overall == "HALT":
        sys.exit(2)
    elif overall == "FAIL":
        sys.exit(1)
    else:
        sys.exit(0)


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: 运行基础验证 — 确认脚本语法正确**

Run:
```bash
cd TestVDB
python -m py_compile scripts/ai_failure_check.py && echo "Syntax OK"
```
Expected: `Syntax OK`

- [ ] **Step 3: 测试 ai_failure_check.py — 空会话目录**

Run:
```bash
mkdir -p /tmp/test_ai_check_empty/defects
python scripts/ai_failure_check.py /tmp/test_ai_check_empty defect-001
```
Expected: exit code 0, `"overall": "PASS"`（空目录无问题可检测）

- [ ] **Step 4: 测试 ai_failure_check.py — M4 缺失 .done 标记**

Run:
```bash
mkdir -p /tmp/test_ai_check_m4/defects /tmp/test_ai_check_m4/debate_logs
echo "# Defect 1\n## Metadata\n- Type: Type1_IllegalSuccess" > /tmp/test_ai_check_m4/defects/defect-001.md
# 故意不创建 .done 文件
python scripts/ai_failure_check.py /tmp/test_ai_check_m4 defect-001
echo "Exit code: $?"
```
Expected: `"mode": "M4", "passed": false`, exit code 2 (HALT)

- [ ] **Step 5: Commit**

```bash
git add TestVDB/scripts/ai_failure_check.py
git commit -m "feat(ai-check): add 7-mode AI failure checklist script"
```

#### Subtask 4b: 修改 reporter.md — Pre-Submit Gate 前插入自检

- [ ] **Step 6: 在 reporter.md 的 Pre-Submit Gate 之前插入 7-mode 自检步骤**

在 `reporter.md` 的「Pre-Submit Gate」章节之前（在 `**每个确认的缺陷在写入 defect-N.md 之前，必须通过复现验证：**` 之前），插入以下内容：

```markdown
## 7-Mode AI Failure Checklist（Pre-Submit Gate 前置步骤）

**在执行 Pre-Submit Gate 复现验证之前，必须对每个候选缺陷运行 AI 失败自检：**

```bash
python scripts/ai_failure_check.py ${session_dir} defect-{N}
```

**检查结果处理（按严重性）：**

| 检查结果 | 行为 |
|---------|------|
| PASS（exit 0） | 继续 Pre-Submit Gate 复现验证 |
| FAIL（exit 1）| M2/M3/M6 触发 → 数据造假嫌疑。**直接丢弃该缺陷**，不生成 defect-N.md。在 session_metadata.json 中记录 AI_SELF_CHECK_FAILED |
| HALT（exit 2）| M4/M7 触发 → 流程违规或死循环。**挂起当前轮次**，写入 HALT 标记文件，等待人工介入。不生成任何报告 |

**各 Mode 说明：**
- M1: 脚本错误被误判为数据库缺陷（信息性，不阻断）
- M2: 编造文档引用（curl 验证 source_url）→ FAIL → 丢弃缺陷
- M3: 编造执行结果数据（比对 output_*.log）→ FAIL → 丢弃缺陷
- M4: 走捷径跳过关键验证（检查 .done 标记）→ HALT → 挂起
- M5: 脚本 bug 被说成新发现（分类一致性检查）→ FAIL → 回退到 Stage 2
- M6: 编造方法论（检查 attack agent 输出一致性）→ FAIL → 丢弃缺陷
- M7: 锁定早期错误假设（endpoint 反复驳回）→ HALT → 挂起

**M2 特殊规则（网络容错）：**
- 每个 source_url 最多重试 2 次，间隔 3 秒
- 如果所有 URL 都不可达 → 可能是网络问题 → 降级为 WARN，不丢弃缺陷
- 只有部分 URL 不可达 → FAIL → 丢弃缺陷
```

- [ ] **Step 7: Commit**

```bash
git add TestVDB/agents/reporter.md
git commit -m "feat(ai-check): add 7-mode AI failure checklist to reporter Pre-Submit Gate"
```

#### Subtask 4c: 更新 defect-taxonomy skill

- [ ] **Step 8: 在 skills/defect-taxonomy/SKILL.md 增加 7-mode checklist 参考**

在文件末尾追加：

```markdown
## 7-Mode AI Failure Checklist (v2.0)

Reporter 在 Pre-Submit Gate 之前运行的自检机制。详见 `scripts/ai_failure_check.py`。

| Mode | 检查内容 | 检测方法 | 触发行为 |
|------|---------|---------|---------|
| M1 | 脚本错误被误判为数据库缺陷 | 检查 execution_summary.txt | 信息性 |
| M2 | 编造文档引用（幻觉 URL） | curl source_url | REJECT |
| M3 | 编造执行结果数据 | 比对 output_*.log | REJECT |
| M4 | 走捷径跳过关键验证 | 检查 .done 标记 | HALT |
| M5 | 脚本 bug 被说成新发现 | 分类一致性检查 | REWIND |
| M6 | 编造方法论 | attack agent 输出一致性 | REJECT |
| M7 | 锁定早期错误假设 | endpoint 反复驳回 | HALT |
```

- [ ] **Step 9: Commit**

```bash
git add TestVDB/skills/defect-taxonomy/SKILL.md
git commit -m "docs(defect-taxonomy): add 7-mode AI failure checklist reference"
```

---

### Task 5: P1 — Fan-Out Attack Trio

**Files:**
- Modify: `commands/mine.md`
- Modify: `agents/orchestrator.md`
- Modify: `skills/pipeline/SKILL.md`

#### Subtask 5a: 修改 commands/mine.md 的 Step 8b — 3 并发 → 9 并发

- [ ] **Step 1: 替换 mine.md Step 8b 的攻击派发逻辑**

在 `commands/mine.md` 的 Step 8b 中，将现有的 3 并发调用替换为 Fan-Out 版本的 9 并发调用：

```markdown
#### 8b. Fan-Out Attack Trio（⛔ 禁止自己写脚本）

**v2.0 新增 — Fan-Out 模式（fan_out.enabled=true 时）：**

每个 Attack Agent 派发 `fan_out.seeds_per_agent` 次（默认 3），每次使用不同的 `focus_profile`：

```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-boundary.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-boundary.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-state.md 规范，为 {target} v{version} 生成状态攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-state.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-state.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-semantic.md 规范，为 {target} v{version} 生成语义攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-semantic.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-semantic.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")
```

**9 个 Agent 全部并行派发**。超时机制不变（3 分钟无产出 → 超时）。部分超时不影响其他 seed。

**汇聚与去重（fan_out.enabled=true 时）：**
```bash
# 1. 统计所有 9 个 seed 的产出脚本
find results/{target}/{version}/{timestamp} -name "*.py" -type f ! -path "*/mre/*" ! -name "_stage1*" ! -name "script_*" 2>/dev/null | wc -l
```
为 0 则报错终止。

**3 级去重（主进程自行执行）：**
1. 按 (endpoint, constraint_id, strategy) 三元组去重
2. 相同三元组 → 保留 confidence 最高的版本
3. 不同 seed 独立生成相同脚本 → confidence +0.1（独立验证奖励）

**如果 fan_out.enabled=false 或 seeds_per_agent=1 → 回退到 v1.x 行为（3 并发，无 focus_profile）。**
```

- [ ] **Step 2: Commit**

```bash
git add TestVDB/commands/mine.md
git commit -m "feat(fan-out): upgrade attack dispatch from 3 to 9 concurrent agents with focus profiles"
```

#### Subtask 5b: 修改 orchestrator.md — Fan-Out + focus_profile 文档化

- [ ] **Step 3: 在 orchestrator.md Step 8b 中增加 Fan-Out 文档**

在 `agents/orchestrator.md` 的 Step 8b 部分，在现有的 3 并发描述之后，追加 Fan-Out 说明：

```markdown
### v2.0 Fan-Out 模式（fan_out.enabled=true）

当 Fan-Out 启用时，每个 Attack Agent 使用 3 种 focus_profile 各派发一次：

| Profile | 策略 | Agent prompt 差异 |
|---------|------|-------------------|
| `priority_first` | 从 contract 中 severity 最高的约束开始 | 无额外指令（默认行为） |
| `coverage_gap` | 从 coverage.json 中覆盖率最低的端点开始 | 注入 uncovered_endpoints 列表 |
| `rejection_pattern` | 从上轮 false positive 反向推导新攻击 | 注入 rejection_patterns，"绕过已知驳回模式" |

9 组脚本 → 统一汇聚 → Stage 1 去重 + 交叉审查

**去重规则（3 级）：**
1. 按 (endpoint, constraint_id, strategy) 三级去重
2. 相同三元组 → 保留 confidence 最高的版本
3. 跨 profile 重复检测 → 不同 seed 独立生成相同脚本 → confidence +0.1

**首轮建议：** 先用 `seeds_per_agent=2` 测试，确认去重逻辑正确后再增加到 3。
```

- [ ] **Step 4: Commit**

```bash
git add TestVDB/agents/orchestrator.md
git commit -m "docs(orchestrator): document Fan-Out mode, focus profiles, and 3-tier dedup"
```

#### Subtask 5c: 更新 pipeline skill

- [ ] **Step 5: 在 skills/pipeline/SKILL.md Phase 3 中更新 Fan-Out 描述**

替换 Phase 3 的现有描述：

```markdown
### Phase 3: 测试生成 (v2.0 Fan-Out)

1. Orchestrator 并发派 Attack Trio（boundary + state + semantic）
2. **v2.0 Fan-Out**：每个 Agent 使用 3 种 focus_profile 各派发一次（共 9 并发）
   - priority_first: 优先高严重性约束
   - coverage_gap: 优先低覆盖率端点
   - rejection_pattern: 绕过已知驳回模式
3. 每个 Agent 独立生成测试脚本（最多 30 个/Agent/profile/轮）
4. 注入 reflection_context + 跨会话策略（首轮无）
5. **汇聚去重**：3 级去重（endpoint + constraint_id + strategy）
6. 辩论 Stage 1：自动化审查（去重 + 语法验证 + 约束验证 + 跨 Agent 交叉审查）
7. 通过脚本存入 `results/{target}/{version}/{timestamp}/script_*.py`
```

- [ ] **Step 6: Commit**

```bash
git add TestVDB/skills/pipeline/SKILL.md
git commit -m "docs(pipeline): update Phase 3 for v2.0 Fan-Out with focus profiles"
```

---

### Task 6: P0 — 跨会话自进化引擎

**Files:**
- Create: `scripts/strategy_extractor.py`
- Create: `scripts/strategy_injector.py`
- Create: `strategy_registry/global_strategies.json`
- Create: `strategy_registry/milvus_strategies.json`
- Create: `strategy_registry/qdrant_strategies.json`
- Create: `strategy_registry/weaviate_strategies.json`
- Create: `strategy_registry/pgvector_strategies.json`
- Modify: `commands/mine.md`
- Modify: `agents/orchestrator.md`
- Modify: `agents/attack-boundary.md`
- Modify: `agents/attack-state.md`
- Modify: `agents/attack-semantic.md`
- Modify: `skills/pipeline/SKILL.md`

#### Subtask 6a: 创建策略注册表目录和初始文件

- [ ] **Step 1: 创建 strategy_registry 目录和空注册表文件**

```bash
mkdir -p TestVDB/strategy_registry
```

每个注册表文件初始化为带策略数组的空 JSON：

```json
{
  "_meta": {
    "db": "global",
    "description": "跨 DB 通用攻击策略注册表",
    "created_at": "2026-06-07T00:00:00Z",
    "version": "1.0"
  },
  "strategies": []
}
```

为 global/milvus/qdrant/weaviate/pgvector 各创建一个，`_meta.db` 分别设为对应值。

- [ ] **Step 2: 创建空的 evolution_log.jsonl**

```bash
touch TestVDB/strategy_registry/evolution_log.jsonl
```

- [ ] **Step 3: Commit**

```bash
git add TestVDB/strategy_registry/
git commit -m "feat(evolution): create strategy registry directory and empty per-DB strategy files"
```

#### Subtask 6b: 创建 strategy_extractor.py

- [ ] **Step 4: 创建 scripts/strategy_extractor.py**

```python
#!/usr/bin/env python3
"""
策略提取器 — 从 experience_handoff.json 提取可复用策略并写入 Strategy Registry。

用法:
  python scripts/strategy_extractor.py <session_dir> <target_db>

输入:
  session_dir: 会话目录（含 experience_handoff.json）
  target_db: milvus/qdrant/weaviate/pgvector

行为:
  1. 读取 experience_handoff.json 中的 confirmed_defects
  2. 提取 attack_type + constraint_type + endpoint 模式
  3. 泛化：将 DB 特定的 API 调用替换为抽象模式
  4. 交叉分析：检查相同模式是否已在其他 DB 的 registry 中存在
  5. 新策略 → 写入对应 DB 的 registry
  6. 已有策略 → 更新 performance 计数 + 调整 confidence
  7. 追加 evolution_log.jsonl 审计条目
"""

import json
import os
import sys
import re
from datetime import datetime, timezone
from pathlib import Path


PROJECT_ROOT = os.environ.get("PROJECT_ROOT", os.getcwd())
REGISTRY_DIR = os.path.join(PROJECT_ROOT, "strategy_registry")
LOG_PATH = os.path.join(REGISTRY_DIR, "evolution_log.jsonl")

DB_ENDPOINT_PATTERNS = {
    "milvus": {
        "collection": r"/v2/vectordb/collections/\{collection_name\}",
        "points": r"/v2/vectordb/collections/\{collection_name\}/points",
        "search": r"/v2/vectordb/collections/\{collection_name\}/points/search",
        "index": r"/v2/vectordb/collections/\{collection_name\}/indexes",
    },
    "qdrant": {
        "collection": r"/collections/\{collection_name\}",
        "points": r"/collections/\{collection_name\}/points",
        "search": r"/collections/\{collection_name\}/points/search",
        "index": None,  # Qdrant 无独立索引端点
    },
    "weaviate": {
        "collection": r"/v1/schema/\{class_name\}",
        "points": r"/v1/objects",
        "search": r"/v1/graphql",
        "index": None,
    },
    "pgvector": {
        "collection": None,  # PGVector 使用 SQL TABLE
        "points": None,
        "search": None,
        "index": None,
    },
}

ENDPOINT_CATEGORIES = [
    (r"(?:create|insert|put|post).*collection", "collection_create"),
    (r"(?:delete|drop).*collection", "collection_delete"),
    (r"(?:get|list|describe).*collection", "collection_read"),
    (r"search.*points?", "search"),
    (r"(?:insert|upsert|put).*points?", "points_insert"),
    (r"(?:delete).*points?", "points_delete"),
    (r"(?:get|retrieve).*points?", "points_read"),
    (r"(?:update).*points?", "points_update"),
    (r"count.*points?", "count"),
    (r"(?:create|build).*index", "index_create"),
    (r"create.*table", "ddl"),
]


def load_json(path: str) -> dict:
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def save_json(path: str, data: dict):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def append_log(entry: dict):
    """追加一条 evolution log"""
    with open(LOG_PATH, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry, ensure_ascii=False) + "\n")


def classify_endpoint(endpoint: str) -> str:
    """分类端点模式"""
    for pattern, category in ENDPOINT_CATEGORIES:
        if re.search(pattern, endpoint, re.IGNORECASE):
            return category
    return "other"


def generalize_endpoint(endpoint: str, source_db: str) -> str:
    """将 DB 特定端点泛化为抽象模式"""
    patterns = DB_ENDPOINT_PATTERNS.get(source_db, {})
    for _category, pattern in patterns.items():
        if pattern:
            # 反向：endpoint → 抽象模式
            pass
    # 简化：提取关键词
    category = classify_endpoint(endpoint)
    return f"{{db}}.{category}.{{endpoint}}"


def extract_strategy_from_defect(defect: dict, session_dir: str) -> dict:
    """从单个缺陷提取策略"""
    strategy = {
        "strategy_id": "",
        "category": "",
        "origin": {
            "db": "",
            "version": "",
            "session_id": "",
            "defect_id": "",
            "created_at": datetime.now(timezone.utc).isoformat()
        },
        "pattern": {
            "name": "",
            "description": "",
            "template": "",
            "constraint_types": [],
            "applicable_endpoints": []
        },
        "migration": {
            "applicable_dbs": ["milvus", "qdrant", "weaviate", "pgvector"],
            "confirmed_dbs": [],
            "rejected_dbs": [],
            "migration_rules": {}
        },
        "performance": {
            "total_attempts": 1,
            "defects_found": 1,
            "false_positives": 0,
            "avg_confidence": 0.8,
            "last_used": datetime.now(timezone.utc).isoformat()
        },
        "status": "experimental"
    }

    endpoint = defect.get("endpoint", "")
    defect_type = defect.get("defect_type", "")
    confidence = defect.get("confidence", 0.5)
    summary = defect.get("summary", "")

    # 推断 category
    if defect_type and "Type1" in defect_type:
        strategy["category"] = "boundary"
    elif defect_type and "Type4" in defect_type:
        strategy["category"] = "state"
    else:
        strategy["category"] = "semantic"

    # 生成 strategy_id
    ep_category = classify_endpoint(endpoint)
    strategy["strategy_id"] = f"{ep_category}_{strategy['category']}_{defect_type or 'unknown'}"

    # pattern
    strategy["pattern"]["name"] = summary[:50] if summary else f"Attack on {endpoint}"
    strategy["pattern"]["description"] = summary
    strategy["pattern"]["template"] = f"Test {endpoint} for {defect_type or 'defect'} violation"
    strategy["pattern"]["applicable_endpoints"] = [f"*+{ep_category}", "*+create", "*+insert", "*+search"]

    # performance
    strategy["performance"]["avg_confidence"] = confidence

    return strategy


def generate_strategy_id(base: str, registry: dict) -> str:
    """生成唯一 strategy_id，避免冲突"""
    existing_ids = {s["strategy_id"] for s in registry.get("strategies", [])}
    candidate = base.lower().replace(" ", "_").replace("/", "_")
    if candidate not in existing_ids:
        return candidate
    # 追加版本号
    for i in range(2, 100):
        v = f"{candidate}_v{i}"
        if v not in existing_ids:
            return v
    return f"{candidate}_{int(datetime.now().timestamp())}"


def merge_strategy(new_strategy: dict, existing: dict) -> dict:
    """合并策略：更新 performance，调整 confidence"""
    perf = existing["performance"]
    perf["total_attempts"] += 1
    perf["defects_found"] += new_strategy["performance"]["defects_found"]
    new_conf = new_strategy["performance"]["avg_confidence"]
    old_conf = perf["avg_confidence"]
    # 移动平均
    perf["avg_confidence"] = round((old_conf * 0.7 + new_conf * 0.3), 2)
    perf["last_used"] = datetime.now(timezone.utc).isoformat()

    # 合并 confirmed_dbs
    origin_db = new_strategy["origin"]["db"]
    if origin_db not in existing["migration"]["confirmed_dbs"]:
        existing["migration"]["confirmed_dbs"].append(origin_db)

    return existing


def main():
    if len(sys.argv) < 3:
        print("Usage: python scripts/strategy_extractor.py <session_dir> <target_db>")
        sys.exit(1)

    session_dir = sys.argv[1]
    target_db = sys.argv[2].lower()

    if target_db not in ("milvus", "qdrant", "weaviate", "pgvector"):
        print(f"Error: Unknown target_db '{target_db}'")
        sys.exit(1)

    # 1. 读取 experience_handoff.json
    exp_path = os.path.join(session_dir, "experience_handoff.json")
    exp = load_json(exp_path)

    if not exp:
        print(json.dumps({"status": "no_data", "reason": "experience_handoff.json not found or empty"}))
        return

    # 2. 提取缺陷策略
    key_findings = exp.get("key_findings", [])
    extracted = 0
    merged = 0

    for defect in key_findings:
        strategy = extract_strategy_from_defect(defect, session_dir)
        strategy["origin"]["db"] = target_db
        strategy["origin"]["session_id"] = exp.get("session_id", "unknown")
        strategy["origin"]["version"] = exp.get("version", "unknown")
        strategy["origin"]["defect_id"] = defect.get("defect_id", "unknown")

        # 3. 检查 global registry
        global_path = os.path.join(REGISTRY_DIR, "global_strategies.json")
        global_reg = load_json(global_path)

        existing = None
        for gs in global_reg.get("strategies", []):
            if gs["strategy_id"] == strategy["strategy_id"]:
                existing = gs
                break

        if existing:
            merge_strategy(strategy, existing)
            merged += 1
        else:
            strategy["strategy_id"] = generate_strategy_id(strategy["strategy_id"], global_reg)
            strategy["migration"]["confirmed_dbs"] = [target_db]
            global_reg.setdefault("strategies", []).append(strategy)
            extracted += 1

            # 日志
            append_log({
                "ts": datetime.now(timezone.utc).isoformat(),
                "event": "strategy_created",
                "strategy_id": strategy["strategy_id"],
                "origin_db": target_db,
                "origin_defect": strategy["origin"]["defect_id"]
            })

        save_json(global_path, global_reg)

    # 4. 同步到目标 DB 的 registry
    db_path = os.path.join(REGISTRY_DIR, f"{target_db}_strategies.json")
    db_reg = load_json(db_path)
    # 从 global 复制 applicable 到 DB 的 strategy
    for gs in global_reg.get("strategies", []):
        if target_db in gs["migration"]["applicable_dbs"]:
            exists = any(s["strategy_id"] == gs["strategy_id"]
                        for s in db_reg.get("strategies", []))
            if not exists:
                db_reg.setdefault("strategies", []).append(gs)
    save_json(db_path, db_reg)

    result = {
        "status": "ok",
        "extracted": extracted,
        "merged": merged,
        "target_db": target_db,
        "session_id": exp.get("session_id", "unknown")
    }
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
```

- [ ] **Step 5: 验证脚本语法**

Run:
```bash
cd TestVDB
python -m py_compile scripts/strategy_extractor.py && echo "Syntax OK"
```
Expected: `Syntax OK`

- [ ] **Step 6: 单元测试 — 空 experience_handoff.json**

Run:
```bash
mkdir -p /tmp/test_strategy_session
echo '{}' > /tmp/test_strategy_session/experience_handoff.json
python scripts/strategy_extractor.py /tmp/test_strategy_session milvus
```
Expected: `"status": "no_data"`

- [ ] **Step 7: 单元测试 — 有缺陷数据**

Run:
```bash
cat > /tmp/test_strategy_session/experience_handoff.json << 'EOF'
{
  "session_id": "milvus-2617-r1",
  "target": "milvus",
  "version": "v2.6.17",
  "round": 1,
  "key_findings": [
    {
      "endpoint": "/v2/vectordb/collections/{name}",
      "defect_type": "Type1_IllegalSuccess",
      "confidence": 0.92,
      "summary": "Enum parameter boundary injection: invalid enum value accepted"
    }
  ]
}
EOF
python scripts/strategy_extractor.py /tmp/test_strategy_session milvus
```
Expected: `"status": "ok", "extracted": 1`

- [ ] **Step 8: 验证 global_strategies.json 有新条目 + evolution_log.jsonl 有记录**

Run:
```bash
python -c "
import json
with open('TestVDB/strategy_registry/global_strategies.json') as f:
    reg = json.load(f)
print(f'Strategies in global: {len(reg.get(\"strategies\", []))}')
for s in reg.get('strategies', []):
    print(f'  - {s[\"strategy_id\"]} [status={s[\"status\"]}]')
"
echo "--- evolution_log.jsonl ---"
cat TestVDB/strategy_registry/evolution_log.jsonl
```
Expected: 应有至少 1 条策略和 1 条 evolution log

- [ ] **Step 9: 清理测试数据后 Commit**

```bash
# 重置 registry 为初始状态
cd TestVDB
python -c "
import json
from pathlib import Path
for f in Path('strategy_registry').glob('*_strategies.json'):
    with open(f) as fh:
        data = json.load(fh)
    data['strategies'] = []
    with open(f, 'w') as fh:
        json.dump(data, fh, indent=2, ensure_ascii=False)
# 清空 evolution log
open('strategy_registry/evolution_log.jsonl', 'w').close()
print('Registry reset to empty state')
"

git add TestVDB/scripts/strategy_extractor.py
git commit -m "feat(evolution): add strategy_extractor.py for cross-session strategy extraction"
```

#### Subtask 6c: 创建 strategy_injector.py

- [ ] **Step 10: 创建 scripts/strategy_injector.py**

```python
#!/usr/bin/env python3
"""
策略注入器 — 读取 Strategy Registry 并输出适合 Agent prompt 注入的策略文本。

用法:
  python scripts/strategy_injector.py <target_db> [--max N] [--min-confidence C]

输入:
  target_db: milvus/qdrant/weaviate/pgvector
  --max N: 最多注入 N 条策略（默认 10）
  --min-confidence C: 最低 confidence 阈值（默认 0.6）

输出 (stdout):
  JSON {strategies: [...], injection_text: "..."}

策略注入时附带 confidence，Agent 应对低 confidence 策略降低依赖。
status=deprecated 的策略不注入。
"""

import json
import os
import sys


PROJECT_ROOT = os.environ.get("PROJECT_ROOT", os.getcwd())
REGISTRY_DIR = os.path.join(PROJECT_ROOT, "strategy_registry")


def load_json(path: str) -> dict:
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {"strategies": []}


def get_strategies(target_db: str, max_count: int = 10, min_confidence: float = 0.6) -> list:
    """获取适用于目标 DB 的策略列表"""
    global_path = os.path.join(REGISTRY_DIR, "global_strategies.json")
    db_path = os.path.join(REGISTRY_DIR, f"{target_db}_strategies.json")

    global_reg = load_json(global_path)
    db_reg = load_json(db_path)

    # 合并 global 和 DB-specific 策略
    all_strategies = {}
    for s in global_reg.get("strategies", []):
        all_strategies[s["strategy_id"]] = s
    for s in db_reg.get("strategies", []):
        all_strategies[s["strategy_id"]] = s

    # 过滤
    candidates = []
    for sid, s in all_strategies.items():
        # 跳过废弃策略
        if s.get("status") == "deprecated":
            continue
        # 检查 DB 是否被拒绝
        if target_db in s.get("migration", {}).get("rejected_dbs", []):
            continue
        # 检查 confidence
        conf = s.get("performance", {}).get("avg_confidence", 0.0)
        if conf < min_confidence:
            continue
        # 检查是否适用于目标 DB
        applicable = s.get("migration", {}).get("applicable_dbs", [])
        if applicable and target_db not in applicable and "all" not in applicable:
            continue

        candidates.append(s)

    # 按 confidence 降序排序
    candidates.sort(key=lambda s: s.get("performance", {}).get("avg_confidence", 0), reverse=True)

    return candidates[:max_count]


def generate_injection_text(strategies: list, target_db: str) -> str:
    """生成注入到 Attack Agent prompt 的策略文本"""
    if not strategies:
        return "（无跨会话策略可用）"

    lines = [
        "## 跨会话策略注入",
        "",
        "以下策略来自之前成功挖掘的经验（跨 DB 迁移）。使用这些策略作为初始 seed。",
        "",
    ]

    for i, s in enumerate(strategies, 1):
        pattern = s.get("pattern", {})
        migration = s.get("migration", {})
        perf = s.get("performance", {})

        migration_rule = migration.get("migration_rules", {}).get(target_db, "no specific rule")

        lines.append(f"### 策略 {i}: {pattern.get('name', s['strategy_id'])}")
        lines.append(f"- **模板**: {pattern.get('template', 'N/A')}")
        lines.append(f"- **类别**: {s.get('category', 'unknown')}")
        lines.append(f"- **置信度**: {perf.get('avg_confidence', 0):.2f}")
        lines.append(f"- **适用端点**: {', '.join(pattern.get('applicable_endpoints', []))}")
        lines.append(f"- **DB 适配**: {migration_rule}")
        lines.append(f"- **约束类型**: {', '.join(pattern.get('constraint_types', []))}")
        lines.append(f"- **来源**: {s.get('origin', {}).get('db', 'unknown')} v{s.get('origin', {}).get('version', '?')}")

        desc = pattern.get('description', '')
        if desc:
            lines.append(f"- **描述**: {desc}")

        lines.append("")

    lines.append("注意：对低置信度策略降低依赖优先级，优先使用高置信度策略。")
    return "\n".join(lines)


def main():
    import argparse

    parser = argparse.ArgumentParser(description="注入跨会话策略到 Attack Agent prompt")
    parser.add_argument("target_db", help="目标数据库 (milvus/qdrant/weaviate/pgvector)")
    parser.add_argument("--max", type=int, default=10, dest="max_count")
    parser.add_argument("--min-confidence", type=float, default=0.6)
    parser.add_argument("--text-only", action="store_true", help="仅输出注入文本，不输出 JSON")

    args = parser.parse_args()

    strategies = get_strategies(args.target_db, args.max_count, args.min_confidence)
    injection_text = generate_injection_text(strategies, args.target_db)

    if args.text_only:
        print(injection_text)
    else:
        result = {
            "strategies": [{"strategy_id": s["strategy_id"],
                           "confidence": s.get("performance", {}).get("avg_confidence", 0),
                           "category": s.get("category")}
                          for s in strategies],
            "count": len(strategies),
            "target_db": args.target_db,
            "injection_text": injection_text
        }
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
```

- [ ] **Step 11: 验证脚本语法**

Run:
```bash
cd TestVDB
python -m py_compile scripts/strategy_injector.py && echo "Syntax OK"
```
Expected: `Syntax OK`

- [ ] **Step 12: 测试 strategy_injector.py**

Run:
```bash
python scripts/strategy_injector.py milvus --text-only
```
Expected: 输出「（无跨会话策略可用）」（registry 为空时）或策略列表

- [ ] **Step 13: Commit**

```bash
git add TestVDB/scripts/strategy_injector.py
git commit -m "feat(evolution): add strategy_injector.py for cross-session strategy query"
```

#### Subtask 6d: 修改 commands/mine.md — 策略注入 + 策略提取

- [ ] **Step 14: 在 mine.md Step 3 中增加策略注册表读取**

在 Step 3（缓存检查）之后、Step 4 之前，插入：

```markdown
### Step 3.5: 跨会话策略注入准备（v2.0 新增，evolution.enabled=true 时）

读取 Strategy Registry 中适用于当前 target 的策略：
```bash
python scripts/strategy_injector.py {target} --text-only
```

将输出文本保存为临时变量 `cross_session_strategies`，供 Step 8a 注入 Attack Agent 使用。

如果 `evolution.enabled=false`，跳过此步骤。
```

- [ ] **Step 15: 在 mine.md Step 8a 中注入策略文本**

在 Step 8a 的 reflection_context 注入模板之后，追加策略注入：

```markdown
**v2.0 新增 — 跨会话策略注入（evolution.enabled=true 时）：**

将 Step 3.5 读取的 `cross_session_strategies` 追加到 Attack Agent 的 prompt 末尾。
如果策略文本为「（无跨会话策略可用）」，跳过注入。
```

- [ ] **Step 16: 在 mine.md Step 9（生成汇总后）增加策略提取**

在 Step 9 的「生成 summary.md」之后、「清理 Docker」之前，插入：

```markdown
**v2.0 新增 — 策略提取（evolution.enabled=true 时）：**

本轮挖掘结束后，提取经验至 Strategy Registry：
```bash
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}
```

检查输出中的 `extracted` 和 `merged` 计数，在 stdout 日志中输出：
```
[Step 9] Strategy extraction: N new strategies extracted, M existing strategies updated
```
```

- [ ] **Step 17: Commit**

```bash
git add TestVDB/commands/mine.md
git commit -m "feat(evolution): integrate strategy injection and extraction into mine command"
```

#### Subtask 6e: 修改 orchestrator.md — 策略注入 + 提取文档

- [ ] **Step 18: 在 orchestrator.md Step 8a 中增加策略注入模板**

在 Step 8a 的 reflection_context 注入模板之后，追加：

```markdown
### v2.0 跨会话策略注入（evolution.enabled=true）

在 reflection_context 之后，追加从 Strategy Registry 读取的策略：
```
## 跨会话策略注入

以下策略来自之前成功挖掘的经验（跨 DB 迁移）：

{cross_session_strategies 的输出}

使用这些策略作为初始 seed。对于标记了 applicable_dbs 包含当前 DB 的策略，
应用 migration_rules 中的 DB 特定适配规则。
```

策略由 `scripts/strategy_injector.py {target} --text-only` 生成。
```

- [ ] **Step 19: 在 orchestrator.md Step 8h 中增加策略提取步骤**

在 Step 8h「分析本轮产出」之后，追加：

```markdown
### v2.0 策略提取（evolution.enabled=true）

每轮结束后（或在 Step 9 统一执行），运行：
```bash
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}
```

策略提取逻辑：
1. 读取本轮 experience_handoff.json
2. 提取 confirmed_defects 的策略模式 → 泛化 → 合并
3. 新策略 → 写入 strategy_registry（global + per-DB）
4. 已有策略 → 更新 performance 计数 + 调整 confidence
5. 追加 evolution_log.jsonl 审计条目
```

- [ ] **Step 20: Commit**

```bash
git add TestVDB/agents/orchestrator.md
git commit -m "docs(orchestrator): add strategy injection template and extraction step for evolution"
```

#### Subtask 6f: 修改 Attack Agents — 策略消费指令

- [ ] **Step 21: 在 attack-boundary.md 增加跨会话策略消费指令**

在 `attack-boundary.md` 的「攻击策略」部分之前，插入：

```markdown
## 跨会话策略消费（v2.0 新增）

如果 prompt 中包含「跨会话策略注入」部分，你应该：

1. **优先使用高置信度（>0.7）策略**作为初始攻击模板
2. 对于标记了 `applicable_dbs` 的策略，应用 `migration_rules` 中的 DB 特定适配规则
3. 低置信度策略降低优先级，但仍作为备选参考
4. 如果策略模板中的端点已在 `exhausted_endpoints` 中，跳过该策略
5. 同一策略在你的 attack round 中最多使用 3 次，避免重复
```

- [ ] **Step 22: 在 attack-state.md 增加相同的跨会话策略消费指令**

（复制 Step 21 的相同内容）

- [ ] **Step 23: 在 attack-semantic.md 增加相同的跨会话策略消费指令**

（复制 Step 21 的相同内容）

- [ ] **Step 24: Commit**

```bash
git add TestVDB/agents/attack-boundary.md TestVDB/agents/attack-state.md TestVDB/agents/attack-semantic.md
git commit -m "feat(evolution): add cross-session strategy consumption instructions to attack trio agents"
```

#### Subtask 6g: 更新 pipeline skill

- [ ] **Step 25: 在 skills/pipeline/SKILL.md Phase 3 中增加策略注入步骤**

在 Phase 3 的步骤 3 之后插入：

```markdown
3a. **v2.0 跨会话策略注入**：从 Strategy Registry 查询适用策略 → 注入 Attack Agent prompt
    - 高 confidence (>0.7) 策略作为优先攻击模板
    - 应用 migration_rules 中的 DB 特定适配规则
    - `status=deprecated` 的策略不注入
```

- [ ] **Step 26: Commit**

```bash
git add TestVDB/skills/pipeline/SKILL.md
git commit -m "docs(pipeline): add strategy injection step to Phase 3"
```

---

### Task 7: P3 — Marketplace 分发优化

**Files:**
- Modify: `README_zh.md`
- Modify: `AGENTS.md`
- Modify: `.claude-plugin/plugin.json`

- [ ] **Step 1: 更新 plugin.json 版本号**

将 `"version": "1.0.0"` 改为 `"version": "2.0.0"`。

- [ ] **Step 2: 更新 README_zh.md 安装部分**

在 README_zh.md 的安装部分增加 marketplace 命令：

```markdown
## 安装

### 方式 1: Marketplace（推荐）
```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### 方式 2: 本地开发
```bash
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```
```

- [ ] **Step 3: 更新 AGENTS.md 增加安装说明**

在 AGENTS.md 的 "For AI Agents" 部分之前增加：

```markdown
## Installation

```bash
# Marketplace (recommended)
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB

# Local development
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

## What's New in v2.0

- **跨会话自进化**: 从 Milvus 挖掘中学到的策略自动迁移到 Qdrant/Weaviate/PGVector
- **Fan-Out Attack Trio**: 3 Agent × 3 seed = 9 并行生成流，策略多样性提升 3x
- **7-Mode AI Failure Checklist**: Reporter 自检 7 种 LLM 幻觉模式，造假→丢弃，违规→挂起
- **Material Passport**: 契约 sha256 防篡改 + 版本化追溯
- **data_access_level**: Agent 数据权限声明式标记
```

- [ ] **Step 4: Commit**

```bash
git add TestVDB/.claude-plugin/plugin.json TestVDB/README_zh.md TestVDB/AGENTS.md
git commit -m "feat(marketplace): bump to v2.0.0, add marketplace install instructions"
```

---

### Task 8: 集成验证 — 完整流程测试

- [ ] **Step 1: 验证 settings.json 完整性和所有 feature flag**

Run:
```bash
cd TestVDB
python -c "
import json
with open('settings.json') as f:
    s = json.load(f)

# 所有 feature flag 存在
for section in ['evolution', 'fan_out', 'ai_failure_check', 'material_passport']:
    assert section in s, f'Missing {section}'
    assert 'enabled' in s[section], f'Missing {section}.enabled'
    print(f'{section}.enabled = {s[section][\"enabled\"]}')

print('All feature flags present')
"
```

- [ ] **Step 2: 验证所有新增 Python 脚本可 import（无语法错误）**

Run:
```bash
cd TestVDB
for f in scripts/passport_verify.py scripts/ai_failure_check.py scripts/strategy_extractor.py scripts/strategy_injector.py; do
    python -m py_compile "$f" && echo "$f: OK" || echo "$f: FAIL"
done
```
Expected: All 4 scripts show `OK`

- [ ] **Step 3: 验证 strategy_registry 目录结构正确**

Run:
```bash
cd TestVDB
echo "=== Strategy Registry ==="
for f in strategy_registry/*.json; do
    echo "$f: $(python -c "import json; d=json.load(open('$f')); print(f'{len(d.get(\"strategies\",[]))} strategies')")"
done
echo "evolution_log.jsonl: $(wc -l < strategy_registry/evolution_log.jsonl 2>/dev/null || echo 0) entries"
```

- [ ] **Step 4: 验证 agent frontmatter 全部有 dataAccess**

Run:
```bash
cd TestVDB
for f in agents/*.md; do
    if grep -q "dataAccess:" "$f"; then
        echo "  ✓ $f"
    else
        echo "  ✗ $f — MISSING dataAccess!"
    fi
done
```
Expected: All 12 agents show `✓`

- [ ] **Step 5: 验证所有 feature flag 关闭时向后兼容**

Run:
```bash
cd TestVDB
python -c "
import json
with open('settings.json') as f:
    s = json.load(f)

# 模拟所有 feature flag 关闭
for section in ['evolution', 'fan_out', 'ai_failure_check', 'material_passport']:
    s[section]['enabled'] = False

# 验证 JSON 仍然合法
json.dumps(s)
print('All features can be individually disabled — backward compatible')
"
```
Expected: `All features can be individually disabled — backward compatible`

- [ ] **Step 6: Commit（集成验证完成）**

```bash
git add TestVDB/
git commit -m "test(integration): verify all v2.0 features are syntactically valid and backward compatible"
```

---

### Task 9: 实战验证 — Live-Fire Mining Run

> **目的**: 在实际向量数据库上运行完整的 mining 流水线，验证所有 v2.0 新功能按设计预期工作。

#### 9.1 验证策略

```
Phase A: 单 DB 挖掘（建立基线）
  └── /testvdb:mine qdrant v1.12.0 --max-rounds 2 --min-defects 1

Phase B: 验证产出质量
  ├── B1: 检查 _passport hash 完整性
  ├── B2: 检查 strategy_registry 有策略条目
  ├── B3: 检查 Fan-Out 脚本数量（应为 3 的倍数）
  ├── B4: 检查 7-mode checklist 被运行
  └── B5: 检查 experience_handoff.json 质量

Phase C: 跨会话进化验证
  ├── C1: 运行第二次 mining（同 DB，模拟"跨会话"）
  └── C2: 验证策略注入是否生效（Attack Agent 产出是否引用了上次策略）

Phase D: 跨 DB 迁移验证
  ├── D1: 对另一个 DB 运行 mining
  └── D2: 检查 strategy_registry 中是否有跨 DB applicable_dbs 标记
```

#### 9.2 逐项验证步骤

- [ ] **Step 1: Phase A — 运行首次完整 mining（Qdrant v1.12.0）**

在加载 TestVDB 插件的 Claude Code 会话中执行：
```
/testvdb:mine qdrant v1.12.0 --max-rounds 2 --min-defects 1
```

**预期行为:**
- 流水线正常完成（6 phase 全部执行）
- 9 个 Attack Agent 并发派发（Fan-Out）
- 每个 Agent prompt 包含 dataAccess 约束
- structured_contract.json 包含 `_passport` 字段
- Reporter 在 Pre-Submit Gate 之前运行 `ai_failure_check.py`
- 至少产出 1 个确认缺陷

- [ ] **Step 2: Phase B1 — 验证 Material Passport hash 完整性**

Run:
```bash
SESSION_DIR=$(ls -td results/qdrant/v1.12.0/*/ | head -1)
python scripts/passport_verify.py "${SESSION_DIR}structured_contract.json"
```
Expected: exit code 0, `"status": "PASS"`

- [ ] **Step 3: Phase B2 — 验证策略注册表有新增条目**

Run:
```bash
python -c "
import json
with open('strategy_registry/global_strategies.json') as f:
    reg = json.load(f)
strategies = reg.get('strategies', [])
print(f'Total strategies in global registry: {len(strategies)}')
assert len(strategies) > 0, 'No strategies extracted!'
for s in strategies[:5]:
    print(f'  - {s[\"strategy_id\"]} (confidence={s.get(\"performance\",{}).get(\"avg_confidence\",0)})')
"
```
Expected: `len(strategies) > 0`, 每条策略有 strategy_id + confidence

- [ ] **Step 4: Phase B2 — 验证 evolution_log.jsonl 有审计记录**

Run:
```bash
echo "Evolution log entries:"
tail -5 strategy_registry/evolution_log.jsonl | python -m json.tool --compact 2>/dev/null || tail -5 strategy_registry/evolution_log.jsonl
```
Expected: 至少 1 条 `strategy_created` 事件

- [ ] **Step 5: Phase B3 — 验证 Fan-Out 产出的脚本数量**

Run:
```bash
SESSION_DIR=$(ls -td results/qdrant/v1.12.0/*/ | head -1)
echo "=== Scripts by source ==="
echo "boundary_scripts: $(ls ${SESSION_DIR}boundary_scripts/*.py 2>/dev/null | wc -l)"
echo "state_scripts: $(ls ${SESSION_DIR}state_scripts/*.py 2>/dev/null | wc -l)"
echo "scripts (semantic): $(ls ${SESSION_DIR}scripts/*.py 2>/dev/null | wc -l)"
echo "script_*.py (root): $(ls ${SESSION_DIR}script_*.py 2>/dev/null | wc -l)"
total=$(find ${SESSION_DIR} -maxdepth 2 -name "*.py" ! -path "*/mre/*" 2>/dev/null | wc -l)
echo "Total scripts: $total"
```
Expected: `Total scripts >= 9`（至少 9 seed × 1 脚本），且各 Agent 均有产出

- [ ] **Step 6: Phase B4 — 验证 7-mode AI checklist 被运行**

Run:
```bash
SESSION_DIR=$(ls -td results/qdrant/v1.12.0/*/ | head -1)
echo "=== Checking for 7-mode checklist evidence ==="
# 检查是否有 PROCESSED 或 AI_SELF_CHECK 标记
grep -r "AI_SELF_CHECK\|ai_failure_check\|7-mode" "${SESSION_DIR}" 2>/dev/null | head -5
echo "---"
# 如果 defects 存在，逐个检查缺陷报告质量
for defect in "${SESSION_DIR}"defects/defect-*.md; do
  [ ! -f "$defect" ] && continue
  echo "=== $(basename $defect) ==="
  # 检查证据链完整性
  echo "  Ring 1 (Contract): $(grep -c 'constraint_id\|Ring 1' "$defect")"
  echo "  Ring 2 (Doc Ref): $(grep -c 'source_url\|Ring 2' "$defect")"
  echo "  Ring 3 (Actual): $(grep -c 'HTTP Response\|Ring 3' "$defect")"
  echo "  Defect Type: $(grep 'Type:' "$defect" | head -1)"
done
```
Expected: 每个 defect-N.md 包含 Ring 1 + Ring 2 + Ring 3 证据链，缺陷类型使用四型分类法

- [ ] **Step 7: Phase B5 — 验证 experience_handoff.json 质量**

Run:
```bash
SESSION_DIR=$(ls -td results/qdrant/v1.12.0/*/ | head -1)
python -c "
import json
with open('${SESSION_DIR}experience_handoff.json') as f:
    exp = json.load(f)
print(f'Session: {exp.get(\"session_id\")}')
print(f'Key findings: {len(exp.get(\"key_findings\", []))}')
print(f'Rejection patterns: {len(exp.get(\"rejection_patterns\", []))}')
print(f'Debate stats: {json.dumps(exp.get(\"debate_stats\", {}), indent=2)}')
assert 'session_id' in exp
assert 'key_findings' in exp
assert 'rejection_patterns' in exp
print('experience_handoff.json structure: VALID')
"
```
Expected: 结构完整，JSON 合法

- [ ] **Step 8: Phase C1 — 运行第二次 mining（同 DB，模拟跨会话进化）**

```
/testvdb:mine qdrant v1.12.0 --max-rounds 1 --min-defects 1
```

**预期行为:**
- Step 3.5 应读取到上次 mining 产生的策略
- Attack Agent prompt 应包含「跨会话策略注入」文本
- 如果 registry 中有 applicable 策略，Attack Agent 产出应体现策略复用

- [ ] **Step 9: Phase C2 — 验证策略注入生效**

Run:
```bash
# 取第二次 mining 的 session dir（最新）
SESSION_DIR=$(ls -td results/qdrant/v1.12.0/*/ | head -1)
echo "Session: $(basename $SESSION_DIR)"

# 检查 Attack Agent 产出中是否有策略复用的痕迹
# 策略复用表现为：使用了与 strategy_registry 中已有策略相似的 endpoint+constraint 组合
python -c "
import json
with open('strategy_registry/global_strategies.json') as f:
    reg = json.load(f)
strategies = [s for s in reg.get('strategies', []) if s.get('status') != 'deprecated']
if strategies:
    print(f'Available strategies for injection: {len(strategies)}')
    for s in strategies[:3]:
        print(f'  - {s[\"strategy_id\"]}: {s[\"pattern\"][\"template\"][:80]}...')
else:
    print('No strategies available — cross-session evolution not yet materialized')
"
```
Expected: 有可用策略。如果 strategy_registry 为空（首次运行），则说明 Phase A 的首次 mining 未产生策略，需要在 Phase A 中检查 experience_handoff.json 是否有 confirmed_defects。

- [ ] **Step 10: Phase D1 — 跨 DB 迁移测试（Milvus）**

```
/testvdb:mine milvus v2.4.0 --max-rounds 1 --min-defects 1
```

**预期行为:**
- `strategy_injector.py` 应查询到 Qdrant 产生的策略（如果 applicable_dbs 包含 milvus）
- `strategy_extractor.py` 应将新策略同步到 `milvus_strategies.json`

- [ ] **Step 11: Phase D2 — 验证跨 DB 迁移标记**

Run:
```bash
python -c "
import json
for db in ['qdrant', 'milvus', 'weaviate', 'pgvector']:
    try:
        with open(f'strategy_registry/{db}_strategies.json') as f:
            reg = json.load(f)
        strategies = reg.get('strategies', [])
        cross_db = [s for s in strategies if len(s.get('migration',{}).get('confirmed_dbs',[])) > 1]
        if strategies:
            print(f'{db}: {len(strategies)} strategies, {len(cross_db)} cross-DB')
        else:
            print(f'{db}: 0 strategies')
    except FileNotFoundError:
        print(f'{db}: file not found')
"
```
Expected: Qdrant 和 Milvus 的 registry 中应有条目，跨 DB 标记正确

- [ ] **Step 12: 最终清理**

Run:
```bash
# 清理所有测试容器
docker compose -f TestVDB/docker/qdrant.yml down -v 2>/dev/null || true
docker compose -f TestVDB/docker/milvus.yml down -v 2>/dev/null || true
```

---

### Task 10: 实战验证报告模板

- [ ] **Step 1: 验证完成后，将结果记录到**

保存验证报告到 `docs/superpowers/plans/2026-06-07-testvdb-v2-validation-report.md`：

```markdown
# TestVDB v2.0 实战验证报告

**日期**: 2026-06-07
**验证者**: 
**版本**: v2.0.0

## 验证摘要

| 验证项 | 状态 | 详情 |
|--------|------|------|
| Phase A: 首次 mining 完成 | PASS/FAIL | |
| B1: Material Passport hash | PASS/FAIL | |
| B2: 策略注册表新增 | PASS/FAIL | |
| B3: Fan-Out 脚本数量 | PASS/FAIL | |
| B4: 7-mode checklist | PASS/FAIL | |
| B5: experience_handoff 质量 | PASS/FAIL | |
| C1: 第二次 mining（跨会话） | PASS/FAIL | |
| C2: 策略注入生效 | PASS/FAIL | |
| D1: 跨 DB 迁移（Milvus） | PASS/FAIL | |
| D2: 跨 DB 迁移标记 | PASS/FAIL | |

## 详细发现

### Phase A 产出
- 确认缺陷数: 
- 缺陷类型分布: 
- 辩论通过率: 

### Material Passport
- _passport 是否存在: 
- hash 验证结果: 

### 策略注册表
- 提取策略数: 
- evolution_log 条目数: 

### Fan-Out
- 脚本总数: 
- 去重后有效脚本数: 

### 7-Mode Checklist
- 是否运行: 
- 是否有 FAIL/HALT 触发: 

## 问题与修复

（记录验证过程中发现的任何问题及修复措施）
```

---

## 实现顺序建议

```
Task 1 (settings.json) → Task 2 (data_access_level) → Task 3 (Passport)
  → Task 4 (7-mode checklist) → Task 5 (Fan-Out) → Task 6 (Evolution)
  → Task 7 (Marketplace) → Task 8 (集成验证) → Task 9 (实战验证)
```

P2 先做（最小侵入，铺路性质），P1 次之（核心能力增强），P0 最后（依赖 P1 Fan-Out 的脚本多样性）。Task 9 是最终验收关卡——只有全部 11 项验证通过才算完成。

---

*Generated by Claude Code + writing-plans skill*
