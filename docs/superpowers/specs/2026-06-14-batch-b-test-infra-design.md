# 批次 B · 测试基础设施 + 契约验证器重写 设计

- **日期**: 2026-06-14
- **批次**: B（健壮性地基）
- **状态**: 无人值守自行执行（用户授权全程判断，核心原则：不动代码库原有记录、可回退）
- **分支**: `feat/batch-b-test-infra`

---

## 1. 背景与范围

批次 A（target neutralization）已合并 main（PR #8/#9）。批次 B 聚焦**测试基础设施**（技术债 #4）+ mine 暴露的 **bug #2**（契约验证器）。bug #3（contract categories）因风险高（涉契约生成）只标注、不修。

### 现状
- 27 个 `scripts/*.py`，仅 4 个 `_test_*.py`（非 pytest 独立脚本：`PASSED/FAILED` + `main()` 返回退出码）
- `tests/` 只有 fixtures，**无 pytest 配置**（无 conftest/pytest.ini/pyproject）
- CLAUDE.md `testing.md` 要求 pytest + 80% 覆盖

### bug #2（`scripts/validate_weaviate_contract.py`）
- 第 11 行硬编码 `results/weaviate/v1.38.0/structured_contract.json`（v 前缀错 + target/version 写死）
- 期望 pre-v2.0 schema（`api_endpoints` list + `constraints` dict + `endpoint_registry` + `_passport`），与现契约结构（`api_endpoint` 单数 / 无 passport）不符
- 第 12 项 `valid_categories` 含 Qdrant 概念（collections/points），与 bug #3 同源
- 是 pre-v2.0 单 DB 验证器，需**重写为通用 contract gate**

### bug #3（contract categories，不修，标注）
- weaviate 契约实测 categories 含 `collections:5, points:17`（Qdrant 概念污染）
- 根因在 `contract-formalizer` 的 category 映射 + `strategy_extractor.generalize_endpoint`
- **风险高**：改它影响未来所有契约生成。无人值守下不动，留专项（需独立 spec + 回归验证）

---

## 2. 自行决策（基于 CLAUDE.md + 最佳实践）

| 决策 | 选择 | 理由 |
|------|------|------|
| 测试框架 | **pytest** | CLAUDE.md testing.md 要求；标准；CI 友好 |
| 现有 `_test_*.py` 处理 | **保留 + pytest 包装**（subprocess 跑 + assert exit 0） | 保留批次 A 既有的 4 个独立测试价值；pytest 统一入口；零重写风险 |
| 新测试风格 | 标准 pytest（test 函数 + assert + fixtures） | 符合规范；可读 |
| bug #2 | **新增通用 `validate_contract.py`**（参数化 target/version，通用 schema 检查），保留旧 `validate_weaviate_contract.py` 不删（可回退） | 不动原有记录原则；新验证器通用 |
| bug #3 | 标注，不修 | 涉契约生成，风险高，留专项 |

---

## 3. 组件设计

### B1-1 · pytest 框架
- `pytest.ini`：testpaths=tests，markers（unit/integration），pythonpath=scripts
- `tests/conftest.py`：共享 fixtures（tmp_session_dir、sample_contract、sys.path 注入 scripts/）
- `tests/__init__.py`

### B1-2 · 包装现有 `_test_*.py`
- `tests/test_existing_scripts.py`：parametrize 4 个 `_test_*.py`，subprocess 跑 + assert exit 0
- 不改原 `_test_*.py`（保留独立可跑）

### B1-3 · 核心脚本 pytest 测试（高价值，聚焦验证类 + 上下文类）
- `tests/test_validate_api_format.py`：AST 检测 bare .json() / safe_request 未调用（fixture 造脚本）
- `tests/test_session_utils.py`：`_plugin_root` / `find_session_id` / `is_session_locked` / .env 引号解析
- `tests/test_strategy_extractor.py`：`generalize_endpoint`（含 pgvector SQL 模式）
- `tests/test_dedup_defects.py`：跨轮次缺陷去重
- `tests/test_passport_verify.py`：Material Passport 哈希验证（有/无 passport）

### B2 · 通用契约验证器
- `scripts/validate_contract.py`（新增，不删旧的）：
  - CLI：`python scripts/validate_contract.py <contract_path>`（参数化，不硬编码 target/version）
  - 通用 schema 检查（适配 v2.0 `api_endpoints` list + 旧 `api_endpoint` 单数）：required fields、endpoint 字段完整、constraint ID 唯一、confidence 范围、source_url 存在、_passport（可选）
  - target-aware category 提示（检测 collections/points 出现在非 qdrant target → 警告，对应 bug #3 的检测侧）
  - 退出码：0 pass / 1 errors / 2 用法错
- `tests/test_validate_contract.py`：多 target（qdrant/weaviate/pgvector）+ 多 schema 版本 fixture

---

## 4. 验收标准

1. `pytest` 一键跑全部测试（含包装的 4 个 _test + 新测试），全绿
2. 现有 `_test_*.py` 仍可独立跑（`python scripts/_test_*.py`）——未破坏
3. `validate_contract.py <任意契约路径>` 通用工作（不硬编码）；旧 `validate_weaviate_contract.py` 保留不删
4. 不动 `results/`/`contracts/`/main 历史；全 commit 可回退
5. bug #3 在 PR 描述标注（留专项）

---

## 5. 不做（YAGNI）

- 不追求 27 脚本 100% 覆盖（聚焦高价值验证类 + 核心，~8 测试文件）
- 不重写 crawl_*/github_search（外部 IO，单测价值低）
- 不修 bug #3（契约生成逻辑，风险高）
- 不引入 coverage 强制门槛（先建框架，coverage 阈值留后续）
