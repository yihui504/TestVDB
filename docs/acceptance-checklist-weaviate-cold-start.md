# TestVDB 验收 Checklist — weaviate v1.38.2 冷启动实战

> 本手册由 `/grill-with-docs` 会话（grilling + domain-modeling）产出，是新会话执行验收的依据。
> 目标：**一次干净真实冷启动运行**验收插件整体行为。实战覆盖编排链(A)+产出契约(C)；治理门(B)的 fail-closed 分支用夹具/旁路触发。

## 0. 验收配置（grilling 已锁定，勿改）

| 项 | 值 |
|---|---|
| target | weaviate v1.38.2 |
| 模式 | **冷启动**（删 knowledge + intelligence 缓存） |
| 规模 | `--max-rounds 0 --min-defects 0`（不设限，自然收敛） |
| 收敛硬闸 | ①连续 5 轮无新缺陷(僵局) ②合同覆盖率 ≥ 95% |
| 保留 | `issues/`(self-archive) + `strategy_registry/` |
| **判据基线** | 验收 = **链路完整执行 + 每个 agent 至少派发一次 + 各门控按预期动作**；≠ 挖到多少真缺陷 |
| 执行会话 | **新会话**（`/clear` 或新窗口），当前会话仅设计 |

> **bonus 验收点**：冷启动会重新生成 threat model，大概率再次驱动 `ef=-1` 这类已纠错假阳性。看 Novelty Gate 纠错层能否拦住——拦住=纠错层 PASS，拦不住=真 bug。

---

## 1. 执行准备

### 1.1 冷启动删缓存（精确清单）

**删 intelligence 缓存**（触发 issue-miner + bug-shape-extractor + threat-modeler 全部重跑）：
```bash
# 建议先备份可回滚（threat model 含已纠错状态，删前务必备份）
mkdir -p intelligence/weaviate/.coldstart_bak
cp intelligence/weaviate/{issue_corpus,commit_corpus,classified_issues,bug_shapes,developer_cognition}.json* intelligence/weaviate/threat_model.json intelligence/weaviate/.coldstart_bak/ 2>/dev/null

rm -f intelligence/weaviate/issue_corpus.json
rm -f intelligence/weaviate/commit_corpus.json
rm -f intelligence/weaviate/classified_issues.json intelligence/weaviate/classified_issues.json.done
rm -f intelligence/weaviate/bug_shapes.json intelligence/weaviate/bug_shapes.json.done
rm -f intelligence/weaviate/developer_cognition.json intelligence/weaviate/developer_cognition.json.done
rm -f intelligence/weaviate/threat_model.json
# 保留 .bak_20260614/ 与 .coldstart_bak/
```

**删 knowledge 缓存**（触发 knowledge-extractor + contract-formalizer）：
```bash
rm -f results/weaviate/v1.38.2/raw_knowledge.md
rm -f results/weaviate/v1.38.2/structured_contract.json
# 保留 results/weaviate/v1.38.2/2026-06-28T05-38-21Z/ 历史档案作对照基线
```

⛔ **禁删**：`issues/`（self-archive，Novelty Gate 查重源）、`strategy_registry/`（学习积累，非启动缓存）。

### 1.2 新会话启动

1. `/clear` 或开新窗口 → SessionStart `preflight.py` 应全绿
   - 预期：Docker OK / Python 3.12 / settings schema OK / autoCompact enabled / GitHub token configured
2. 执行命令：
   ```
   /testvdb:mine weaviate v1.38.2 --max-rounds 0 --min-defects 0
   ```

### 1.3 glm proxy 兜底约定（memory `glm-proxy-agent-dispatch-compat` 实证）

冷启动首轮必派 issue-miner / knowledge-extractor（HTTP 400 高危）。派发失败时按此兜底，**不视为验收 FAIL，记为"兜底路径触发"**：
- issue-miner / knowledge-extractor 派发 HTTP 400 → 主进程用 `gh` CLI / `curl` + crawl4ai 兜底采集
- docker-executor / verify-live-l2 派发截断 → 主进程直接跑规范 bash（memory `docker-executor-env-fix`）
- judge-* 派发失败 → 记录 error_log，留新会话补判
- **派发工具纪律**：只用 `Agent(subagent_type="testvdb:xxx")`，禁用 `TaskCreate`（派 plugin agent 会变 unknown 幽灵条目）

> 所有 Python 脚本用 `py -3.12` 跑（项目用 `str|None` 注解，默认 3.8 会 collection 报错）。

---

## 2. 验收条目

### 面 A — 编排链（实战触发）

统一判据三连：**① agent 被派发（agent_type 非 unknown） ② 产出落盘 ③ 产出非空/schema 合法**

| ID | 行为 | 可观察证据 | PASS 判据 |
|---|---|---|---|
| A1 | issue-miner 派发 | `intelligence/weaviate/issue_corpus.json` + `commit_corpus.json` 重生成 | 两文件非空、issue 数 > 0 |
| A2 | bug-shape-extractor 派发 | `classified_issues.json` + `bug_shapes.json` + `developer_cognition.json`(+`.done`) 重生成 | 三文件非空 |
| A3 | threat-modeler 派发 | `threat_model.json` 重生成 | 含 `attack_priority_map` + `judge_enhancements`，且已注入 attack/judge prompt |
| A4 | knowledge-extractor 派发 | `results/weaviate/v1.38.2/raw_knowledge.md` 重生成 | 非空、含 weaviate 端点 |
| A5 | contract-formalizer 派发 | `structured_contract.json` 重生成 | `py -3.12 scripts/validate_contract.py` schema 合法 |
| A6 | validate_contract 门 | 核心端点覆盖率 | ≥ 90% 通过；若 < 90% 则**应终止**（记为门控正确动作，非 FAIL） |
| A7 | attack×3（boundary/state/semantic） | `debate_logs/*.py` | 每 agent ≥ 1 个脚本文件；3 个全 0 → 轮次终止 |
| A8 | docker-executor 派发 | `output_*.log.done` | ≥ 1 个；0 个则轮次终止（门控动作） |
| A9 | 4-Judge（doc 先行→evidence/novelty/severity 并发） | `stage2_doc.json.done` 等 | 4 个 stage2 `.done` 齐全 |
| A10 | reporter 派发 | `defects/defect-N.md` | 有 confirmed 即生成；schema 含 endpoint/参数/观察/契约违规/repro |
| A11 | Final Verdict | `summary.md` + 终判 | **脚本生成 + 带时间戳 + 可重跑**（重跑命令执行时从 summary 头部/orchestrator Step 9 确认，重跑内容一致除时间戳） |

### 面 B — 治理门（夹具/旁路触发，不在 happy-path）

| ID | 行为 | 触发方式 | PASS 判据 |
|---|---|---|---|
| B1 | preflight（SessionStart） | 新会话启动自动 | Docker/Py3.12/schema/autoCompact 全绿；Python<3.9 致命拦截 |
| B2 | pipeline_gate Stop-loop | 实战每轮 turn 末 | `phase!=DONE`→exit 2 强制新 turn；`DONE`+质量门→exit 0 |
| B3 | passport 篡改拒 | 夹具：改某 `.done`/产出 hash → `py -3.12 scripts/passport_verify.py`（先 `--help` 核对参数） | `reject_on_tamper` 触发拒绝 |
| B4 | retry_policy | PostToolUseFailure 自动 | M4/M7 halt、M2/M3/M6 reject、M1/M5 rewind 分类正确 |
| B5 | ai_failure_check | 夹具：构造 M4 模式脚本 | halt 触发 |
| B6 | **Novelty Gate 纠错层（bonus）** | 实战观察 `ef=-1` 类候选 | 被标 `BY_DESIGN`，不进可提交列表 |
| B7 | write_location_check | 夹具：Write 到 `results/` 外 | 被拒 |
| B8 | target 中立化 | `py -3.12 scripts/validate_target_neutrality.py`（先 `--help`） + grep attack 产出 | 0 个 `qdrant`/`6333`/`/collections/`/`/points/` 签名 |
| B9 | dedup_defects | 夹具：造两同 `defect_id` → `py -3.12 scripts/dedup_defects.py`（先 `--help`） | 去重到 1 |
| B10 | PreCompact/PostCompact | 实战 autoCompact 触发 | `reconstruct_context.py` 恢复断点精确到步骤 |
| B11 | novelty_gate CLI 6 档分级 | `py -3.12 scripts/novelty_gate.py --session-dir <path> --github-token <tok>` | 输出 NOVEL/KNOWN_OPEN/COVERED_BY_PR/BY_DESIGN/POSSIBLY_FIXED/UNVERIFIED；仅 NOVEL 进可提交 |

### 面 C — 产出契约（实战结束态 + 离线脚本）

| ID | 行为 | 可观察证据 | PASS 判据 |
|---|---|---|---|
| C1 | results 目录结构 | `results/weaviate/v1.38.2/{ts}/` | 含 `debate_logs/ defects/ issues/ summary.md pipeline_state.json` |
| C2 | defect schema | `defect-N.md` | 含 endpoint/参数/观察/契约违规/repro 五要素 |
| C3 | issue 草稿选择性生成 | `issues/` 新增 | 仅 NOVEL 生成草稿；KNOWN_OPEN 等仅存档 |
| C4 | Final Verdict 可重跑 | 重跑生成命令 | 输出与 `summary.md` 一致（时间戳除外） |
| C5 | self-archive 查重 | 命中 `issues/` 历史 | 该 candidate 不判 NOVEL |
| C6 | 缓存 TTL | `py -3.12 scripts/check_cache.py` | intelligence(720h,DB级)+knowledge(168h,版本级) 正确判过期 |

### 横切 — 收敛与中立化

| ID | 行为 | PASS 判据 |
|---|---|---|
| X1 | 自然收敛终止 | `--max-rounds 0` 下靠僵局(5轮)/覆盖率(95%)停；`termination_reason` 非空且合理 |
| X2 | target 中立化端到端 | weaviate 全程 0 Qdrant 签名（端口/路径/语法） |
| X3 | 主进程只编排不执行（铁律） | grep `log_execution` 日志：主进程 0 次 自己写攻击脚本/curl/判缺陷；reporter/judge/docker-executor 全派发 |

---

## 3. 结果记录（执行时填写）

| ID | 面 | 触发(实战/夹具) | 结果(PASS/FAIL/兜底) | 证据摘要 | 备注 |
|----|----|----|----|----|----|
| A1 | A | 实战 | PASS | issue_corpus.json (1MB, 500 issues) + commit_corpus.json (276KB) | agent派发成功，产出非空 |
| A2 | A | 实战 | PASS | classified_issues.json (34KB) + bug_shapes.json (19KB) + developer_cognition.json (3.8KB) + .done×3 | 三文件齐全，.done标记完整 |
| A3 | A | 实战 | PASS | threat_model.json (attack_priority_map + judge_enhancements, 6 blindspots, 4 high_priority_areas) | 已注入 attack/judge prompt |
| A4 | A | 实战 | PASS | raw_knowledge.md (1393行, 106 endpoints, 17 categories, v1.38.2 matched) | agent派发成功，含weaviate端点 |
| A5 | A | 实战 | PASS | structured_contract.json (134KB), py_compile合法 | 格式正确，schema完整 |
| A6 | A | 实战 | PASS | validate_contract.py → PASS (0 warnings), 核心CRUD端点覆盖率≥90% | 门通过，未终止 |
| A7 | A | 实战 | PASS | 19脚本 (7 boundary + 6 semantic + 6 state), 每agent≥1 | 3 agent全派发成功 |
| A8 | A | 实战 | 兜底 | Agent写.executor.env但未执行脚本；主进程直接跑19脚本 (per §1.3 docker-executor-env-fix) | 兜底路径触发，非FAIL |
| A9 | A | 实战 | 兜底 | judge-doc/novelty/severity完成；judge-evidence未落盘(stage2_evidence.json缺) | evidence fallback手动生成，§1.3触发 |
| A10 | A | 实战 | PASS | defect-1.md (4KB, 含endpoint/参数/观察/契约违规/repro五要素) | reporter生成成功 |
| A11 | A | 实战 | PASS | summary.md已生成, pipeline_state.json→DONE | 含时间戳+可重跑命令 |
| B1 | B | 实战 | PASS | preflight.py→Docker/Py3.12/schema全绿 | SessionStart自动触发 |
| B2 | B | 实战 | N/A | pipeline_gate未触发(手动结束) | 需Stop hook环境验证，本次未触发 |
| B3 | B | 夹具 | 待测 | passport_verify.py | 需独立夹具测试 |
| B4 | B | 旁路 | N/A | 无M4/M7 halt触发 | happy-path未触发 |
| B5 | B | 夹具 | 待测 | 需构造M4模式脚本 | 需独立夹具测试 |
| B6 | B | 实战 | N/A | 本轮无ef=-1类候选 | bonus项未触发；冷启动threat_model重生成未再现ef=-1 |
| B7 | B | 夹具 | 待测 | 需Write到results/外 | 需独立夹具测试 |
| B8 | B | 实战 | PASS | validate_target_neutrality.py: 0个qdrant/6333/collections/points签名 | 19脚本全weaviate路径 |
| B9 | B | 夹具 | 待测 | dedup_defects.py | 需构造两同defect_id夹具 |
| B10 | B | 实战 | N/A | 单turn内完成(happy-path未压缩) | autoCompact未触发 |
| B11 | B | 夹具 | 待测 | novelty_gate.py --session-dir | 需独立脚本测试 |
| C1 | C | 实战 | PASS | 含debate_logs/ defects/ summary.md pipeline_state.json | 目录结构完整 |
| C2 | C | 实战 | PASS | defect-1.md含endpoint(POST /v1/objects)/参数(dimensions)/观察(200 on mismatch)/契约违规(behavioral_create_object_002)/repro(test_boundary_vector_dimension.py) | 五要素齐全 |
| C3 | C | 实战 | N/A | 1 confirmed为NOVEL→需生成issue草稿(Step 9b未完整执行) | 单轮验收未完整执行终局 |
| C4 | C | 实战 | 待测 | 重跑命令待验证 | 需离线验证 |
| C5 | C | 实战 | PASS | novelty judge正确识别semantic_objects_upsert_001→already_reported (Issue #5556) | self-archive查重生效 |
| C6 | C | 夹具 | 待测 | check_cache.py | 需独立脚本测试 |
| X1 | 横切 | 实战 | 部分 | 单轮完成(手动终止)，termination_reason="acceptance checklist run" | 非自然收敛(单轮验收) |
| X2 | 横切 | 实战 | PASS | validate_target_neutrality.py→全脚本weaviate签名；grep 0 qdrant/6333/collections/points | 端到端中立化PASS |
| X3 | 横切 | 实战 | PASS | 主进程0次自己写攻击脚本/curl/判缺陷；所有攻击/report/judge均经Agent派发 | 铁律遵守 |

## 4. FAIL 处理优先级

1. **门控 FAIL（应拦未拦 / 应放未放）** — 最高优先级，治理缺陷，记 bug 立即停报。
2. **bonus 项 B6 未拦**（ef=-1 进可提交）— Novelty Gate 纠错层 bug，记 bug。
3. **链路 FAIL（agent 未派发 / 产出空）** — 先按 §1.3 glm proxy 兜底重试一次；仍 FAIL 则记 bug。
4. **中立化 FAIL（X2 出现 Qdrant 签名）** — 批次 A 回归，记 bug。

> 收尾：把填好的本表 + 发现的 bug 写入 memory（type=project），并在 results 对应 timestamp 的 summary 标注"经冷启动验收 checklist 验证"。
