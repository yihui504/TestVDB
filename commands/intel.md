---
description: 单独采集指定 DB 的历史 Issue/Commit 情报并构建威胁模型
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:intel — 情报采集 + 威胁建模

单独为指定向量数据库采集历史 Issue/Commit 情报，构建威胁模型与认知盲点（`threat_model.json`）。**只跑情报**，不跑契约/攻击/执行。用于刷新过期情报、跨 DB 迁移前更新威胁模型、单独调试 threat-modeler。

> 情报是 **per-target**（不限 version）——同一 DB 的所有版本共享一份 `intelligence/{target}/threat_model.json`。

---

## ⚠️ 架构约束（CRITICAL — 技术原因）

**与 `/testvdb:mine` 相同：主进程永远只做编排，不做执行。**

| 禁止事项 | 正确做法 |
|---------|---------|
| ❌ 自己爬取 GitHub Issues/Commits | ✅ `Agent(subagent_type="testvdb:issue-miner")` |
| ❌ 自己分类/提取 bug shape | ✅ `Agent(subagent_type="testvdb:bug-shape-extractor")` |
| ❌ 自己构建威胁模型 | ✅ `Agent(subagent_type="testvdb:threat-modeler")` |

主进程只使用 `Read`/`Write`/`Bash`(验证)/`Grep`/`Glob`/`Agent` 做编排。

---

## Usage

```
/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, 或 `pgvector` |
| `--max-issues N` | No | settings.json `intelligence.max_issues`（默认 500） | 采集最近 N 条 issue + 已合并 PR |
| `--max-commits N` | No | settings.json `intelligence.max_commits`（默认 200） | 采集最近 N 个 commit |
| `--force` | No | — | 强制重新采集，忽略缓存 |

---

## 执行步骤

### Step 1: 解析参数 + 前置检查

- 验证 `target` ∈ {milvus, qdrant, weaviate, pgvector}
- 解析 `max_issues`、`max_commits`、`force`
- 确定 `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`
- 前置检查：`python scripts/preflight.py`
- 若 `intelligence.enabled=false`（settings.json）→ 提示"情报功能未启用"并退出

### Step 2: 读取 intelligence 配置（CLI 参数覆盖默认）

```bash
python -c "
import json
with open('settings.json', encoding='utf-8') as f:
    c = json.load(f).get('intelligence', {})
print(f'INTEL_TW={c.get(\"time_window_months\", 24)}')
print(f'INTEL_MI={c.get(\"max_issues\", 500)}')
print(f'INTEL_MC={c.get(\"max_commits\", 200)}')
print(f'INTEL_TTL={c.get(\"cache_ttl_hours\", 720)}')
"
```

> **CLI 覆盖**：`--max-issues N` → `INTEL_MI=N`；`--max-commits N` → `INTEL_MC=N`。未传则用 settings 默认值。

### Step 3: 缓存检查（D 判断，批次 D）

检查 `intelligence/{target}/threat_model.json` 是否可复用：

```bash
python scripts/check_cache.py intel "intelligence/{target}" {target} --ttl {INTEL_TTL}
```

- **USABLE**（exit 0）且**未传 `--force`** → 跳到 [Step 7: 输出](#step-7-输出)（报告"缓存有效，跳过采集"）
- **STALE / INVALID / MISSING** 或 **传了 `--force`** → 继续 Step 4 重新采集

> TTL 默认 720h（30 天）。

### Step 4: 派 issue-miner

```
Agent(subagent_type="testvdb:issue-miner",
  description="采集 {target} 历史 Issues 和 Commits",
  prompt="按照 agents/issue-miner.md 规范，为 {target} 采集历史 Issues 和已合并修复 PR。输入参数: target={target}, version=*, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}。将结果写入 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json。")
```

> `version=*` 表示采集全版本历史（情报是 per-target，不限版本）。若失败 → 记录警告，跳过 Step 5/6。

### Step 5: 派 bug-shape-extractor

```
Agent(subagent_type="testvdb:bug-shape-extractor",
  description="提取 {target} 历史 Bug Shapes",
  prompt="按照 agents/bug-shape-extractor.md 规范，对 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json 进行分类和根因模式提取。将结果写入 intelligence/{target}/classified_issues.json、bug_shapes.json、developer_cognition.json。")
```

### Step 6: 派 threat-modeler

```
Agent(subagent_type="testvdb:threat-modeler",
  description="构建 {target} 威胁模型",
  prompt="按照 agents/threat-modeler.md 规范，基于 bug_shapes.json、classified_issues.json、developer_cognition.json 构建威胁模型。将结果写入 intelligence/{target}/threat_model.json。")
```

### Step 7: 输出

加载情报摘要：

```bash
python -c "
import json
with open('intelligence/{target}/threat_model.json', encoding='utf-8') as f:
    tm = json.load(f)
print(json.dumps({
    'blindspot_count': len(tm.get('cognitive_blindspots', {}).get('blindspots', [])),
    'high_priority_areas': [a['area'] for a in tm.get('attack_surface', {}).get('high_priority_areas', [])],
    'top_blindspots': [b['blindspot_id'] for b in tm.get('cognitive_blindspots', {}).get('blindspots', [])[:3]],
}, indent=2, ensure_ascii=False))
" 2>/dev/null || echo "THREAT_MODEL_NOT_AVAILABLE"
```

报告：
- 情报路径：`intelligence/{target}/threat_model.json`
- 盲点数（`blindspot_count`）、高优先攻击面（`high_priority_areas`）、Top 3 盲点
- 采集规模：`max_issues={INTEL_MI}`、`max_commits={INTEL_MC}`、时间窗 `{INTEL_TW}` 月
- 来源：缓存复用 / 新采集

---

## 独立性

本命令**只跑情报采集 + 建模**，不启动：
- ❌ 文档提取/契约生成（→ 用 `/testvdb:contract`）
- ❌ 攻击生成/执行（→ 用 `/testvdb:mine`）

典型用途：
1. **情报刷新**：`--force` 强制重新采集过期情报
2. **跨 DB 迁移前**：为新目标 DB 构建威胁模型
3. **单独调试**：验证 issue-miner/bug-shape/threat-modeler 链路
4. **规模调整**：`--max-issues 50 --max-commits 20` 快速采集小样本

---

## 与 /testvdb:mine 的关系

`/testvdb:mine` 的情报阶段（智能消费）在缓存缺失/过期时调用与本命令**完全相同**的 agent 派发逻辑（issue-miner → bug-shape-extractor → threat-modeler）。本命令是 mine 情报阶段的**独立可触发版本**。详见 `commands/mine.md` Step 3.6。
