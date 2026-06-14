# bug #3 · 契约 category 去 Qdrant 倾向（Category Neutralization）设计

- **日期**: 2026-06-14
- **类型**: bug 修复（bug #3，批次 B 标注的专项）
- **分支**: `feat/bug3-category-neutralization`
- **原则**: 不动原有记录（`results/`/`strategy_registry/`/旧验证器保留），全 commit 可回退

---

## 1. 问题

契约 `api_endpoints[].category` Qdrant 中心化。实测 weaviate 契约：`/v1/schema`→`collections`、`/v1/objects`→`points`（应为 schema/objects）。与批次 A 同源——分类逻辑也 Qdrant 倾向。

### 根因（两层）
- **层 1（生成端）**：`contract-formalizer.md` 规则 2.5 + 第 112-114 行强制所有 DB 端点映射到 Qdrant 倾向"标准分类"词表（`collections/points/search/index/management/ddl/dml/dql`），映射表把 weaviate objects→points、schema→collections。
- **层 2（消费端）**：`strategy_extractor.py` 的 `DB_ENDPOINT_PATTERNS` 键名用 `collection/points` 命名所有 DB 资源（weaviate schema 键名 collection、objects 键名 points）；`ENDPOINT_CATEGORIES` 正则用 `collection_*/points_*/ddl` 词。

### 影响面（全链路 5 处，范围 2）
1. `agents/contract-formalizer.md` 规则 2.5 + 第 112-114（生成端词表）
2. `scripts/strategy_extractor.py` DB_ENDPOINT_PATTERNS 键名 + ENDPOINT_CATEGORIES 正则（消费端）
3. `agents/attack-boundary.md` 第 159-160 占位符（`category=collections/points` 引用速查表）—— 批次 A 遗留
4. `scripts/validate_contract.py` bug #3 检测（适配新词表）
5. `scripts/validate_weaviate_contract.py` 旧验证器（**保留不动**，可回退）

---

## 2. 方案：通用 target 中立词表（方向 A）+ 全链路（范围 2）

category 是**功能分组标签**（下游 strategy_extractor 匹配/优先级用），不需 per-DB 精确。用一套 DB 无关的通用功能语义分类。

### 通用词表（6 类）

| 通用类 | 语义 | 旧 Qdrant 倾向词 → 映射 |
|--------|------|------------------------|
| `schema` | 结构定义/管理（create/drop collection/class/schema/table） | `collections`, `collection`, `ddl` |
| `data` | 记录读写（insert/get/delete objects/points/entities/rows） | `points`, `vector`, `vectors`, `entities`, `entity`, `dml` |
| `search` | 检索（search/query/graphql/recommend） | `query`, `recommend`, `dql` |
| `index` | 索引管理（create/drop index） | `indexes`, `indices` |
| `admin` | 运维管理（cluster/snapshot/backup/shard/partition/health/stats/modules/vacuum/alias/system） | `management`, `partition`, `alias`, `cluster`, `admin`, `system` |
| `other` | 兜底（罕见、无法按功能归类） | — |

各 DB 特有端点归属（验证通用词表覆盖力）：partitions/segments/shards/snapshots/cluster/modules/vacuum → `admin`；graphql/recommend → `search`；references → `data`；ddl→schema/dml→data/dql→search。

---

## 3. 各文件改动

### 3.1 `agents/contract-formalizer.md`
- **第 112-114** category 描述：标准分类改为 `schema/data/search/index/admin/other`，去掉 Qdrant 倾向词。
- **规则 2.5（313-345）**：标准分类名 + 映射表全改为通用词表（上表）。`collection→schema, points→data, vector→data, ddl→schema, dml→data, dql→search, ...`。

### 3.2 `scripts/strategy_extractor.py`
- **DB_ENDPOINT_PATTERNS（34-58）**：键名 `collection→schema`, `points→data`（**路径正则不变**，只改键名）。`search`/`index` 键名保留（已是通用）。
- **ENDPOINT_CATEGORIES（61-75）**：正则分类改通用词：`collection_create→schema_create`, `points_insert→data_insert`, `ddl→schema_create`, etc。`search`/`index_create`/`count` 保留或调通用。

### 3.3 `agents/attack-boundary.md` 第 159-160
占位符 category 引用改通用：`category=collections → category=schema`，`category=points → category=data`。

### 3.4 `scripts/validate_contract.py`
bug #3 检测（91-100）：从"检测 collections/points 污染"改为"**校验 category 在通用词表内**"——非 `{schema,data,search,index,admin,other}` → 警告（既检测旧 Qdrant 词残留，也检测任何非通用词）。

### 3.5 旧 `validate_weaviate_contract.py`
**保留不动**（可回退，不动原有记录）。

---

## 4. 现有数据保留（不动原有记录）

- `results/**/*.structured_contract.json`：旧 category（collections/points）保留，**不重新生成**。
- `strategy_registry/`：旧策略键名保留；新策略自然用通用词（旧策略可能匹配不上新 category，但不破坏）。
- 旧 `validate_weaviate_contract.py`：保留。

---

## 5. 回归测试

- `tests/test_strategy_extractor.py`：适配新通用词（classify_endpoint/generalize_endpoint 断言 schema/data 而非 collections/points）。
- `tests/test_validate_contract.py`：bug #3 检测适配（通用词表校验，非通用词 → 警告）。
- grep 验证：contract-formalizer 规则 2.5 + attack-boundary 占位符用通用词。
- 现有 `pytest` 全过（37 passed 不回归）。

**契约生成端到端验证（可选，需 mine/2.1.165）**：跑 `/testvdb:mine weaviate` 看新契约 category 是否通用（schema/data 而非 collections/points）。留作合并后验证。

---

## 6. 验收标准

1. contract-formalizer 规则 2.5 + 112-114 用通用词表（schema/data/search/index/admin/other）
2. strategy_extractor DB_ENDPOINT_PATTERNS 键名 + ENDPOINT_CATEGORIES 正则用通用词
3. attack-boundary 占位符（159-160）用通用 category（schema/data）
4. validate_contract 检测通用词表（非通用词 → 警告）
5. 现有 results/strategy_registry/旧验证器不动
6. pytest 全过（含适配的新测试）
