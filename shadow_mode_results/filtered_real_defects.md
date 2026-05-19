# Milvus v2.6.16 筛选后的真实缺陷清单

**生成时间：** 2026-05-19
**目标版本：** milvusdb/milvus:v2.6.16
**数据来源：** shadow_mode_results/mine_defects.json (96) + batch_baseline.json (24)
**已排除：** 已提交的5个Issue (#49823, #49824, #49844, #49889, #49890)

---

## 筛选方法论

### 排除规则
1. 已提交Issue的重复缺陷（#49823 nprobe=0, #49824 重复集合名, #49844 null filter, #49889 dbName="", #49890 Request-Timeout非integer）
2. Mine模式boundary测试中的search端点参数注入（~60个）：Milvus REST代理静默忽略未识别JSON键是预期行为，不是"接受非法值"
3. Mine模式missing_required测试：移除的参数对search端点是可选的，成功是预期行为
4. Mine模式mutation测试：脚本因JSONDecodeError崩溃，无法判定是否为真实缺陷
5. IDEMPOTENT_SUCCESS类（drop不存在资源）：REST API幂等删除是常见设计选择
6. has_nonexistent类：返回 `{code:0, value:false}` 是正确的REST语义

### 保留标准
- P0：安全风险、数据丢失、REST/SDK行为不一致、资源耗尽
- P1：API规范违反、参数校验缺失、状态转换违规
- P2：边界条件、边缘case

---

## P0 — 必须提交（5个）

### 1. 32768维集合创建可致OOM

| 字段 | 内容 |
|------|------|
| 缺陷名称 | resource_large_dimension |
| 缺陷类型 | 资源耗尽 / 拒绝服务 |
| 影响端点 | `POST /v2/vectordb/collections/create` |
| 期望行为 | 应拒绝超过合理上限的维度值，返回参数校验错误 |
| 实际行为 | 32768维集合创建成功，可能导致服务端OOM |
| 接受率预估 | 95% |
| **建议Issue标题** | `[REST API] Creating collection with dimension=32768 succeeds, potential OOM/DoS risk` |

**复现步骤：**
```python
import requests
BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={
    "collectionName": "test_large_dim",
    "schema": {
        "autoID": False,
        "enableDynamicField": True,
        "fields": [
            {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
            {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 32768}}
        ]
    }
})
print(r.json())  # code=0, 创建成功
```

---

### 2. REST与SDK创建索引行为不一致

| 字段 | 内容 |
|------|------|
| 缺陷名称 | diff_create_index |
| 缺陷类型 | REST/SDK行为不一致 (DIFFERENTIAL_MISMATCH) |
| 影响端点 | `POST /v2/vectordb/indexes/create` (REST) vs `client.create_index()` (SDK) |
| 期望行为 | REST和SDK对同一操作应返回一致的结果 |
| 实际行为 | REST返回成功(code=0)，SDK抛出异常 |
| 接受率预估 | 90% |
| **建议Issue标题** | `[REST API] create_index returns success via REST but fails via PyMilvus SDK for same operation` |

**复现步骤：**
```python
# REST API - 成功
import requests
BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
# 创建集合（不含indexParams）
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={
    "collectionName": "test_idx",
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]}
})
# 创建索引
r = requests.post(f'{BASE}/v2/vectordb/indexes/create', headers=HEADERS, json={
    "collectionName": "test_idx",
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "IVF_FLAT", "params": {"nlist": 128}}]
})
print(r.json())  # code=0

# PyMilvus SDK - 失败
from pymilvus import MilvusClient, Collection, FieldSchema, CollectionSchema, DataType
client = MilvusClient(uri="http://localhost:19530", token="root:Milvus")
# 同样操作，SDK抛出异常
```

---

### 3. 重复ID插入后计数返回-1

| 字段 | 内容 |
|------|------|
| 缺陷名称 | sequence_6 |
| 缺陷类型 | 数据完整性 / 状态不一致 (SEQUENCE_VIOLATION) |
| 影响端点 | `POST /v2/vectordb/entities/insert` |
| 期望行为 | 重复ID插入应返回准确的插入计数（0或1），或返回明确的重复错误 |
| 实际行为 | 返回count=-1，这是无效的计数值 |
| 接受率预估 | 85% |
| **建议Issue标题** | `[REST API] Inserting duplicate ID returns count=-1 instead of valid insert count` |

**复现步骤：**
```python
import requests, uuid
BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'test_dup_' + uuid.uuid4().hex[:8]
# 创建集合并插入
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
# 第一次插入
r1 = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={
    "collectionName": c, "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
# 重复ID插入
r2 = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={
    "collectionName": c, "data": [{"id": 1, "vector": [0.5, 0.6, 0.7, 0.8]}]
})
print(r2.json())  # insertCount=-1
```

---

### 4. Create-Drop-Create后维度信息丢失

| 字段 | 内容 |
|------|------|
| 缺陷名称 | state_create_drop_create_dim |
| 缺陷类型 | 状态转换违规 / 数据丢失 (SEQUENCE_VIOLATION) |
| 影响端点 | `POST /v2/vectordb/collections/create`, `drop`, `describe` |
| 期望行为 | 重新创建集合后，describe应返回正确的维度信息 |
| 实际行为 | dim返回None，内部状态未正确清理或重建 |
| 接受率预估 | 85% |
| **建议Issue标题** | `[REST API] Collection dimension returns None after create-drop-create cycle` |

**复现步骤：**
```python
import requests
BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
# Step 1: 创建dim=8的集合
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={
    "collectionName": "test_cdc", "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 8}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
# Step 2: 删除集合
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName": "test_cdc"})
# Step 3: 重新创建同名dim=8的集合
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={
    "collectionName": "test_cdc", "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 8}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
# Step 4: describe集合
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName": "test_cdc"})
print(r.json())  # dim=None
```

---

### 5. 超大维度值在集合创建时无上限校验

| 字段 | 内容 |
|------|------|
| 缺陷名称 | create_mutation_oversized_dimension |
| 缺陷类型 | 参数校验缺失 / 资源耗尽 |
| 影响端点 | `POST /v2/vectordb/collections/create` |
| 期望行为 | 应拒绝超大维度值，返回参数范围错误 |
| 实际行为 | 超大维度值被接受，可能导致内存过度分配 |
| 接受率预估 | 80%（建议与缺陷1合并为一个Issue） |
| **建议Issue标题** | `[REST API] No upper bound validation for dimension parameter in collection creation` |

**注：** 与缺陷1（32768维OOM）可合并为一个Issue，缺陷1侧重DoS风险，此缺陷侧重通用校验缺失。

---

## P1 — 建议提交（7个）

### 6. nprobe=-1被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | search_nprobe_negative |
| 缺陷类型 | 参数校验缺失 |
| 影响端点 | `POST /v2/vectordb/entities/search` |
| 期望行为 | nprobe为负数时应被拒绝 |
| 实际行为 | nprobe=-1被接受并执行搜索 |
| 接受率预估 | 85%（建议作为#49823的补充评论） |
| **建议Issue标题** | `[REST API] Negative nprobe value (-1) accepted in search without validation` |

---

### 7. 负数TTL被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | alter_invalid_ttl |
| 缺陷类型 | 参数校验缺失 / 语义违反 |
| 影响端点 | `POST /v2/vectordb/collections/alter` |
| 期望行为 | TTL为负数无语义意义，应被拒绝 |
| 实际行为 | 负数TTL被接受，可能导致集合立即过期或行为未定义 |
| 接受率预估 | 80% |
| **建议Issue标题** | `[REST API] Negative TTL value accepted in collection alter, may cause unexpected data expiration` |

---

### 8. 超大插入数据被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | insert_mutation_oversized_data |
| 缺陷类型 | 参数校验缺失 / 资源耗尽 |
| 影响端点 | `POST /v2/vectordb/entities/insert` |
| 期望行为 | 应对单次插入数据量设置合理上限 |
| 实际行为 | 超大插入数据被接受 |
| 接受率预估 | 70% |
| **建议Issue标题** | `[REST API] No size limit validation for insert data payload` |

---

### 9. 未知参数被静默接受（Permissive Parsing）

| 字段 | 内容 |
|------|------|
| 缺陷名称 | create_mutation_unknown_param + create_mutation_extra_fields |
| 缺陷类型 | API规范违反 / Permissive Parsing |
| 影响端点 | `POST /v2/vectordb/collections/create` 及其他端点 |
| 期望行为 | 应拒绝包含未知参数的请求，或至少返回警告 |
| 实际行为 | 未知参数被静默忽略，请求成功执行 |
| 接受率预估 | 65%（部分维护者可能认为这是设计选择） |
| **建议Issue标题** | `[REST API] Unknown/unexpected parameters silently ignored instead of returning validation error` |

---

### 10. collectionName="" 在创建集合时被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | collectionName_empty_string |
| 缺陷类型 | 参数校验缺失 |
| 影响端点 | `POST /v2/vectordb/collections/create` |
| 期望行为 | 空字符串不是合法的集合名称，应被拒绝 |
| 实际行为 | 空字符串集合名被接受 |
| 接受率预估 | 75% |
| **建议Issue标题** | `[REST API] Empty string accepted as collectionName in create collection` |

---

### 11. 空collectionName在别名列表中被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | alias_list_empty_name |
| 缺陷类型 | 参数校验缺失 |
| 影响端点 | `POST /v2/vectordb/aliases/list` |
| 期望行为 | 空字符串不是合法的集合名称，应返回参数校验错误 |
| 实际行为 | 空字符串被接受 |
| 接受率预估 | 70% |
| **建议Issue标题** | `[REST API] Empty collectionName accepted in aliases/list endpoint` |

---

### 12. db list接受无效参数

| 字段 | 内容 |
|------|------|
| 缺陷名称 | db_list_invalid_param |
| 缺陷类型 | 参数校验缺失 |
| 影响端点 | `POST /v2/vectordb/databases/list` |
| 期望行为 | 无效参数应被拒绝 |
| 实际行为 | 无效参数被接受 |
| 接受率预估 | 65% |
| **建议Issue标题** | `[REST API] Invalid parameters accepted in databases/list endpoint` |

---

## P2 — 可选提交（2个）

### 13. 空ID数组在get操作中被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | get_empty_ids |
| 缺陷类型 | 边界条件 |
| 影响端点 | `POST /v2/vectordb/entities/get` |
| 期望行为 | 空ID数组应被拒绝或返回空结果 |
| 实际行为 | 空ID数组被接受 |
| 接受率预估 | 55% |
| **建议Issue标题** | `[REST API] Empty ID array accepted in entities/get without validation` |

---

### 14. 边界浮点数据（NaN/Inf）在插入时被接受

| 字段 | 内容 |
|------|------|
| 缺陷名称 | insert_mutation_boundary_float_data |
| 缺陷类型 | 边界条件 |
| 影响端点 | `POST /v2/vectordb/entities/insert` |
| 期望行为 | 边界浮点值（如NaN、Inf）应被拒绝 |
| 实际行为 | 边界浮点值被接受，可能导致向量计算未定义行为 |
| 接受率预估 | 60% |
| **建议Issue标题** | `[REST API] Boundary float values (NaN/Inf) accepted in vector insert without validation` |

---

## 提交策略

### 第一批（立即提交，P0）
缺陷1-5，共5个Issue。涉及安全风险、数据完整性和行为不一致。

**合并建议：** 缺陷1和缺陷5合并为一个Issue（维度上限校验缺失），最终4个独立Issue。

### 第二批（P0确认后提交，P1）
缺陷6-12，共7个Issue。缺陷6（nprobe=-1）建议作为#49823的补充评论而非独立Issue。

### 第三批（视反馈决定，P2）
缺陷13-14，共2个Issue。

### 预计提交数量
- 独立Issue：约10-12个
- 预计被接受：8-10个

---

## 不建议提交的缺陷及理由

| 缺陷 | 不提交理由 |
|------|----------|
| drop_nonexistent_index/partition/database/collection | IDEMPOTENT_SUCCESS是REST API的常见设计选择，使DELETE操作幂等是合理的设计意图 |
| coll_has_nonexistent / part_has_nonexistent | `has`操作返回`{code:0, value:false}`是正确的REST语义，不是缺陷 |
| get_nonexistent_ids | 查询不存在的ID返回空结果是合理的，类似SQL的空结果集 |
| Mine中所有search端点boundary测试（offset/dim/nlist/efconstruction等~60个） | 参数注入在search请求体顶层，Milvus静默忽略未识别键是预期行为，不是"接受非法值" |
| Mine中所有missing_required测试 | 移除的参数对search端点是可选的，成功是预期行为 |
| Mine中所有mutation测试 | 测试脚本因JSONDecodeError崩溃，无法判定是否为真实缺陷 |
