# TestVDB v2.1.1 Quality Hardening — 端到端实战验证清单

> **分支**: `feat/v2.1.1-quality-hardening`
> **基线**: `ebeb36f` (origin/main 合并前)
> **已合并 PR**: #4, #5 → origin/main 已包含所有提交
> **未提交变更**: 16 文件 (+409/-193)   |   **未跟踪新文件**: 5 脚本 + 测试固件
> **生成时间**: 2026-06-10

---

## 一、工程化重构 — 脚本提取与 DRY 改造 🔧

这些是从 `commands/mine.md` 代码块中提取为独立 Python 脚本的改造，是 **零脚本错误 (zero script errors)** 目标的核心支撑。

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 1.1 | **风险脚本检测提取** | 已提交 | `scripts/detect_risky_scripts.py` (新, 87行) | 执行 `python scripts/detect_risky_scripts.py <session_dir>` — 验证输出正确标记 RISKY_SCRIPT，与原来内联 Python 代码块逻辑等价 |
| 1.2 | **脚本错误扫描提取** | 已提交 | `scripts/scan_script_errors.py` (新, 81行) | 执行 `python scripts/scan_script_errors.py <session_dir>` — 验证 JSON 输出含 `errored_count` + `scripts`，格式与 `commands/mine.md` Step 8d.5 期望一致 |
| 1.3 | **缺陷去重提取** | 已提交 | `scripts/dedup_defects.py` (新, 117行) | 执行 `python scripts/dedup_defects.py <session_dir>` — 验证跨轮去重逻辑 (endpoint+type 维度)，输出去重后的 `stage2_deduped.json` |
| 1.4 | **威胁模型注入提取** | 已提交 | `scripts/threat_model_injector.py` (新, 256行) | 关键脚本。验证 3 种 `--mode`: `attack` / `judge --judge-type severity` / `--judge-type novelty` / `--judge-type evidence`，输出格式正确且 `--text-only` 返回可注入的纯文本 |
| 1.5 | **Session 工具函数提取** | 已提交 | `scripts/_session_utils.py` (新, 94行) | 验证 `find_session_id()`, `is_session_locked()`, `_plugin_root()` 在以下 8 个脚本中的 import 正常：`cleanup_stop`, `emergency_cleanup`, `log_execution`, `notify_check`, `postcompact_verify`, `precompact_save`, `retry_policy` |

---

## 二、威胁模型程序化注入 (v2.1 Strategic Intelligence) 🧠

由 **硬编码模板** → **程序化注入脚本** 的架构升级。这是 quality hardening 最核心的变更。

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 2.1 | **Attack Agent 注入** | 未提交 | `orchestrator.md`, `mine.md`, `attack-boundary.md`, `attack-semantic.md`, `attack-state.md` | 端到端流程：`threat_model_injector.py --mode attack` 生成文本 → 追加到 Attack Agent prompt 末尾 → 验证 Agent 实际消费了 4 段注入内容（攻击面优先级、认知盲点、by-design 规避、策略权重） |
| 2.2 | **Judge-Severity 校准注入** | 未提交 | `orchestrator.md`, `mine.md`, `judge-severity.md` | 验证 `--mode judge --judge-type severity` 输出含 AUTO_DOWNGRADE / CONFIRM_SEVERITY / DOWNGRADE 规则，且在 Judge 评估时生效 |
| 2.3 | **Judge-Novelty 上下文注入** | 未提交 | `orchestrator.md`, `mine.md`, `judge-novelty.md` | 验证 `--mode judge --judge-type novelty` 输出含已修复模式列表 + 已知进行中 issue + 回归风险区域，且 novelty Judge 消费了这些信息 |
| 2.4 | **Judge-Evidence 成功率注入** | 未提交 | `orchestrator.md`, `mine.md`, `judge-evidence.md` | 验证 `--mode judge --judge-type evidence` 输出含提交成功率 + 证据门槛调整建议，且在 evidence 审查时按高/中/低成功率调整了 Grade |
| 2.5 | **threat_model.json 缺失降级** | 未提交 | `mine.md` Step 8a | 当 `intelligence/<target>/threat_model.json` 不存在时，`threat_model_injector.py` 返回 "(威胁模型数据不可用)"，流水线不中断 |

---

## 三、Agent 架构修正 🔄

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 3.1 | **Agent 数量从 18→16+2** | 未提交 | `README.md`, `README_zh.md` | 确认 `plugin.json` 注册了 16 个 Agent 类型，`orchestrator-lifecycle.md` 和 `reporter-mre.md` 为辅助规范（非独立 Agent） |
| 3.2 | **Reporter 拆分 (reporter + reporter-mre)** | 已提交 | `agents/reporter.md`, `agents/reporter-mre.md` (新) | MRE 脚本生成从 reporter 中分离为独立 Agent (`testvdb:reporter-mre`)，验证 `plugin.json` 注册正确，Orchestrator 在 Step 9a 后正确派发 |
| 3.3 | **Orchestrator 生命周期规范化** | 已提交 | `agents/orchestrator-lifecycle.md` (新) | 验证生命周期管理规则（错误分级重试、PreCompact/PostCompact 上下文保护、进度可见性 `mine_state.json`、多 DB 并行建议）在 orchestrator 中可被正确引用 |

---

## 四、投票逻辑修正 (v2.2 修正) ⚖️

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 4.1 | **Novelty 投票行为修正** | 未提交 | `orchestrator.md` | 旧逻辑：novelty 永远投 is_defect。新逻辑：`already_reported` / `known_wontfix` → not_defect（直接丢弃），`new` / `new_similar` / `unknown` → is_defect |
| 4.2 | **Novelty 超时降级** | 未提交 | `orchestrator.md` | 网络不可用时 novelty 全部标记 `unknown`，投 is_defect（不因网络问题丢弃缺陷） |
| 4.3 | **缺陷确认规则 5 条优先级** | 未提交 | `orchestrator.md` | 验证判定链：evidence → severity → novelty → doc 的顺序和 fallback 行为 |

---

## 五、Judge Agent 强化 📋

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 5.1 | **evidence: Turn 预算扩展** | 未提交 | `judge-evidence.md` | 2 turns → 4 turns，候选 > 10 或跨脚本验证 → 6 turns |
| 5.2 | **severity: Turn 预算扩展** | 未提交 | `judge-severity.md` | 2 turns → 4 turns，v2.1 校准或跨端点 → 5 turns |
| 5.3 | **severity: v2.1 校准规则** | 未提交 | `judge-severity.md` | 4 种校准动作：AUTO_DOWNGRADE_TO_TRIVIAL / AUTO_DOWNGRADE_TO_P3 / CONFIRM_SEVERITY / DOWNGRADE_TO_P3 |
| 5.4 | **novelty: v2.1 上下文消费** | 未提交 | `judge-novelty.md` | 跳过已修复模式、跳过已知 issue、提升回归风险优先级、搜索策略影响 |
| 5.5 | **evidence: v2.1 成功率校准** | 未提交 | `judge-evidence.md` | 高成功率(>0.8) → 降低门槛，低成功率(<0.4) → 提高门槛，中等(0.4-0.8) → 标准 |

---

## 六、脚本质量增强 🐍

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 6.1 | **ai_failure_check: curl → urllib** | 未提交 | `scripts/ai_failure_check.py` | 跨平台无 curl 依赖。验证 M2 模式 URL 可达性检测正常，HTTP 200/301/302 识别正确 |
| 6.2 | **ai_failure_check: M5 扩展** | 未提交 | `scripts/ai_failure_check.py` | Type1-Type4 各类型的证据验证规则完整：Type1 需 2xx，Type2 需错误消息引用，Type3 需 5xx/crash，Type4 需 2xx+状态描述 |
| 6.3 | **ai_failure_check: REWIND 退出码** | 未提交 | `scripts/ai_failure_check.py` | exit 3 (REWIND, M1/M5 触发) → 区分于 exit 1 (FAIL) 和 exit 2 (HALT) |
| 6.4 | **ai_failure_check: M2 全不可达修正** | 未提交 | `scripts/ai_failure_check.py` | 所有 URL 不可达时 → FAIL（而非 PASS），注明可能是编造 URL 或网络问题 |
| 6.5 | **validate_api_format: 扩展检测** | 未提交 | `scripts/validate_api_format.py` | 从仅检测 `requests.xxx().json()` → 检测所有 `.json()` 调用。safe_request() 内部调用为 safe harbor |
| 6.6 | **validate_api_format: 退出码修正** | 未提交 | `scripts/validate_api_format.py` | 有 bare .json() reject → exit 1；仅 warn → exit 0 |
| 6.7 | **passport_verify: 不支持的算法** | 未提交 | `scripts/passport_verify.py` | `compute_hash()` 抛出 ValueError → 返回 `UNSUPPORTED_ALGORITHM` 而非崩溃 |
| 6.8 | **verify_defects: FALSE_POSITIVE → NEEDS_IMPROVEMENT** | 未提交 | `scripts/verify_defects.py` | 脚本错误不再自动判为误报，改为 NEEDS_IMPROVEMENT（可能共存真实缺陷） |
| 6.9 | **verify_defects: 正则修复** | 未提交 | `scripts/verify_defects.py` | `[a-z_0-9]+` → `[\w-]+` (支持连字符的脚本名) |
| 6.10 | **verify_defects: exit 3** | 未提交 | `scripts/verify_defects.py` | 无数据可验证时 exit 3（区别于 PASS） |
| 6.11 | **model-test: 路由说明修正** | 未提交 | `agents/model-test.md` | 从硬编码 "sonnet tier" → "available tier — actual routing depends on environment" |
| 6.12 | **hook_runner: 健壮性** | 已提交 | `scripts/hook_runner.py` | 新增 `cwd=script_dir` 支持相对导入，扩展异常捕获（PermissionError, OSError），脚本不存在时提前报错 |

---

## 七、流水线命令重构 — `commands/mine.md` 📜

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 7.1 | **SOP 职责分离** | 未提交 | `commands/mine.md` | `mine.md` 只保留编排调度，详细 SOP 指向 `orchestrator.md` + `skills/pipeline/SKILL.md` |
| 7.2 | **程序化注入替换硬编码** | 未提交 | `commands/mine.md` Step 8a | 旧：内联 threat model 文本模板。新：调用 `threat_model_injector.py` 生成注入文本 |
| 7.3 | **TimeZone 解析鲁棒性** | 未提交 | `commands/mine.md` Step 4b | `datetime.fromisoformat()` 兼容 Python 3.8-3.10 (Z → +00:00)，fallback 到 `strptime` |
| 7.4 | **内联 Python → 独立脚本** | 已提交 | `commands/mine.md` Step 8c/8d.5/8f | `detect_risky_scripts.py`, `scan_script_errors.py`, `dedup_defects.py` 替换了所有内联代码块 |
| 7.5 | **Issue 审核提醒 (v2.1.2)** | 未提交 | `commands/mine.md` Step 9a.5 | 5 项人工审核清单（最新版本确认、复现步骤、重复检查、格式调整、AI 标记移除） |

---

## 八、基础配置与文档 📖

| # | 验收项 | 变更类型 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|---------|
| 8.1 | **settings.json** | 已提交 | `settings.json` (新) | 验证 settings.json 内容有效（session/preCompact/postCompact/env 配置） |
| 8.2 | **.gitignore 补充** | 未提交 | `.gitignore` | 忽略 `*.log`, `contract_gate.log`, `false_positive_filter.log`, `run_log*.txt`, `mine_*log*.txt`, `mine_*.log`（但不忽略 `docker/*.yml`） |
| 8.3 | **README 双语同步** | 未提交 | `README.md`, `README_zh.md` | 中英文 Agent 数量说明一致（16+2），架构描述准确 |
| 8.4 | **Code Review 报告** | 已提交 | `docs/reviews/code-review-report.md` (158行), `v2.2.md` (181行), `v2.3.md` (519行) | 验证报告内容完整（缺陷分类、修复建议、严重性评级） |
| 8.5 | **威胁模型老化检测** | 已提交 | `commands/mine.md` Step 4b | `threat_model.json` 年龄 > 24h → WARNING 提示重新生成 |

---

## 九、新增未跟踪文件 🆕

| # | 验收项 | 涉及文件 | 验证要点 |
|---|--------|---------|---------|
| 9.1 | **Python 包初始化** | `scripts/__init__.py` (新) | 验证 `from _session_utils import ...` 在 8 个 hooks 脚本中正常工作 |
| 9.2 | **测试固件完整性** | `tests/fixtures/` (新) | `test_crawled_pages.json`, `test2_crawled_pages.json`, `test3_crawled_pages.json`, `test3_contract.json` — 验证格式正确且可被测试代码正确加载 |

---

## 十、端到端集成验证 🎯

这些是最关键的 **全链路实战验证**，需要在实际 DB 环境上运行完整流水线。

| # | 验收项 | 前置条件 | 期望结果 |
|---|--------|---------|---------|
| 10.1 | **完整流水线 cold start** | Milvus/Qdrant 容器运行，`intelligence/<target>/threat_model.json` 存在 | `/testvdb:mine <target> <version>` 完整执行无中断（Step 1→9c），输出 summary.md + defect-review.md |
| 10.2 | **威胁模型注入生效** | 同 10.1 | Attack Agent 日志中可见 `# Blindspot: BS-0x` 标注，Judge 评估中可见校准规则影响 |
| 10.3 | **threat_model.json 缺失降级** | 删除 `intelligence/<target>/threat_model.json` | 流水线不中断，Attack Agent 收到 "(威胁模型数据不可用)" |
| 10.4 | **脚本错误检测准确率** | 故意注入有错误的攻击脚本 | `detect_risky_scripts.py` 正确标记 → `scan_script_errors.py` 正确捕获 → 错误脚本被拒绝重写，不进入缺陷判定 |
| 10.5 | **Novelty already_reported 丢弃** | 模拟 known issue 匹配 | 缺陷被标记 `already_reported` → 投票 `not_defect` → 不生成报告 → dedup_log.json 记录关联 issue |
| 10.6 | **退出码语义正确** | 各脚本在实际场景下运行 | `ai_failure_check.py`: REWIND=3 / FAIL=1 / HALT=2 / PASS=0；`validate_api_format.py`: REJECT=1 / WARN=0；`verify_defects.py`: no_data=3 |
| 10.7 | **Reporter 拆分工作流** | 流水线产出了 confirmed defects | reporter 生成 `defect-N.md` → reporter-mre 生成 `mre/defect-N-script.py` → 编译通过 → `.done` 标记 |

---

## 统计概览

| 层级 | 文件数 | 行变更 | 状态 |
|------|--------|--------|------|
| 已合并提交 (PR#4, #5) | 39 | +2169/-869 | ✅ 已合入 main |
| 未提交工作树 | 16 | +409/-193 | ⚠️ 待提交 |
| 未跟踪新文件 | 5 脚本 + tests/ | ~600 行 | 🆕 待 `git add` |

---

## 验收优先级

1. **必须先验**: 10.1（全链路 cold start）— 通过后才继续
2. **核心逻辑**: 1.x（脚本提取不破坏原有行为）+ 4.x（投票逻辑不引入回归）
3. **质量增强**: 6.x（脚本质量增强）+ 2.x（威胁模型注入正确性）
4. **收尾**: 9.x（新文件就绪）+ 8.x（文档配置一致性）
