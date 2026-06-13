# 批次 A · 验收清单（Target Neutralization）

- **日期**: 2026-06-13
- **分支**: `feat/batch-a-target-neutralization`
- **Spec**: `docs/superpowers/specs/2026-06-13-batch-a-target-neutralization-design.md`
- **Plan**: `docs/superpowers/plans/2026-06-13-batch-a-target-neutralization.md`

---

## 验收标准（spec 第 7 节）

| # | 验收标准 | 实现 | 验证 | 状态 |
|---|---------|------|------|------|
| 1 | `reconstruct_context.py --format text` 输出含「端点速查表」section，列出当前 target 端点 | 组件 C | `_test_reconstruct_context.py` 9/9 | ✅ |
| 2 | 三个 attack agent 示例区零硬编码 | 组件 #1（Task 3/4/5）+ #3（Task 6） | grep：6333/localhost/`/collections/` 仅命中第 19 行禁令列表；weaviate URL 零残留 | ✅ |
| 3 | `_target_api_reference.md` 含唯一 safe_request 权威定义；三 agent 引用而非重定义 | 组件 #1a（Task 2） | `grep -rl "def safe_request" agents/` → 仅 `_target_api_reference.md`（+ 范围外 reporter-mre） | ✅ |
| 4 | `validate_target_neutrality.py` 存在并通过单测；mine.md Stage 1 接入 | 组件 B（Task 7） | `_test_validate_target_neutrality.py` 5/5；mine.md 8c 第 6 步 | ✅ |
| 5 | `pipeline_gate.py` 空 analyzed + ATTACK_GEN completed → exit 2 | 组件 #2'（Task 8） | `_test_pipeline_gate.py` 9/9（含新 case 4b） | ✅ |
| 6 | weaviate 端到端 run 生成的脚本不含 Qdrant 签名 | 端到端 | 待跑 `/testvdb:mine weaviate 1.38.0`（需 Docker） | ⏳ 待验证 |

---

## 组件完成清单

| 组件 | 内容 | commit 主题 |
|------|------|------------|
| C | reconstruct_context 注入端点速查表 | feat(contract): inject target endpoint cheatsheet |
| #1a | safe_request 统一到 _target_api_reference.md | refactor(agents): unify safe_request |
| #1 (boundary) | attack-boundary 示例去 Qdrant 硬编码 | refactor(attack-boundary): de-qdrantize |
| #1 (state) | attack-state 示例去 Qdrant 硬编码 | refactor(attack-state): de-qdrantize |
| #1 (semantic) | attack-semantic 示例 + filter 语法去 Qdrant | refactor(attack-semantic): de-qdrantize + filter |
| #3 | analyzed_documents 示例去 weaviate URL | refactor(agents): replace hardcoded weaviate URLs |
| B | validate_target_neutrality.py + Stage 1 集成 | feat(validate): target-aware neutrality validator |
| #2' | gate 空声明绕过修复 | fix(gate): block empty-statement bypass |

---

## 测试汇总

```
scripts/_test_reconstruct_context.py           → 9/9 passed   (组件 C)
scripts/_test_validate_target_neutrality.py    → 5/5 passed   (组件 B)
scripts/hooks/_test_pipeline_gate.py           → 9/9 passed   (组件 #2'，含新 case 4b)
```

全部测试遵循项目 `_test_*.py` 约定（独立脚本 + PASSED/FAILED + 自造 tempfile fixture，不依赖 gitignored 的 `results/`）。

---

## 范围外项（记录，批次 A 不处理）

- **`agents/reporter-mre.md:56` 的 `def safe_request`**：二元组、用 `DB_URL`/`HEADERS`。reporter-mre 是 MRE 脚本生成器，非攻击 agent；`validate_api_format.py:25` 与 `validate_target_neutrality.py` 均跳过 `/mre/` 目录。→ 移交**批次 B**（测试基础设施）一并考虑统一。
- **gate 说明文字中的 weaviate URL 示例**（如"`https://weaviate.io/developers/weaviate` ≠ `https://docs.weaviate.io/weaviate`"）：保留——这是 gate 精确比对机制的**教学性举例**，非 Agent 照抄的模板示例。

---

## 待办

- [ ] **验收项 6 端到端**：`/testvdb:mine weaviate 1.38.0 --max-rounds 1`，确认生成脚本端口/路径来自 weaviate 契约（8080 + `/objects`），不含 Qdrant 的 6333/`/collections/.../points`；`validate_target_neutrality` 不 REJECT；gate 不因空声明放行。
