---
name: docker-templates
description: TestVDB Docker 容器模板参考。当 Executor Agent 需要启动目标向量数据库容器时自动加载。
version: 1.0.0
---

# Docker Templates Reference

## 触发条件

Docker Executor Agent 需要启动目标 DB 容器时自动加载。非用户手动触发。

## 模板概览

| Template | File | Containers | Complexity |
|----------|------|-----------|------------|
| Milvus | `docker/milvus.yml` | 3 (etcd + MinIO + standalone) | High |
| Qdrant | `docker/qdrant.yml` | 1 | Low |
| Weaviate | `docker/weaviate.yml` | 1 | Low |
| PGVector | `docker/pgvector.yml` | 1 | Low |

## 版本覆盖

通过环境变量覆盖默认版本：

```bash
export MILVUS_VERSION="v2.4.0"
export QDRANT_VERSION="v1.13.0"
export WEAVIATE_VERSION="1.25.0"
export PGVECTOR_VERSION="pg17"
```

## 端口配置

通过环境变量覆盖默认端口：

```bash
export MILVUS_PORT="19530"
export MILVUS_GRPC_PORT="19531"
export QDRANT_PORT="6333"
export QDRANT_GRPC_PORT="6334"
export WEAVIATE_PORT="8080"
export WEAVIATE_GRPC_PORT="50051"
export PGVECTOR_PORT="5432"
```

## 启动命令

```bash
# Milvus (most complex — requires etcd + MinIO first)
docker compose -f docker/milvus.yml up -d
# Wait ~60s for all 3 services to be healthy

# Qdrant
docker compose -f docker/qdrant.yml up -d
# Wait ~15s

# Weaviate
docker compose -f docker/weaviate.yml up -d
# Wait ~15s

# PGVector
docker compose -f docker/pgvector.yml up -d
# Wait ~10s
```

## 健康检查

| DB | Health Endpoint | Expected |
|----|----------------|----------|
| Milvus | `curl -f http://localhost:9091/healthz` | `{...}`, exit 0 |
| Qdrant | `curl -f http://localhost:6333/healthz` | `healthz check passed`, exit 0 |
| Weaviate | `curl -f http://localhost:8080/v1/.well-known/ready` | —, exit 0 |
| PGVector | `pg_isready -U postgres -d testvdb` | accepting connections |

## 启动超时

| DB | Max Wait | Notes |
|----|----------|-------|
| Milvus | 120s | etcd + MinIO + standalone all need to be healthy |
| Qdrant | 30s | Single container, fast |
| Weaviate | 30s | Single container |
| PGVector | 20s | PostgreSQL initialization |

## 清理

```bash
# Normal cleanup
docker compose -f docker/{db}.yml down -v

# Emergency cleanup
docker rm -f testvdb-{db}-standalone testvdb-{db}-etcd testvdb-{db}-minio
```

## SDK 隔离

每个 DB 容器只安装其对应 SDK，禁止交叉污染：

| DB | SDK | Install Command |
|----|-----|----------------|
| Milvus | pymilvus | `pip install pymilvus` |
| Qdrant | qdrant-client | `pip install qdrant-client` |
| Weaviate | weaviate-client | `pip install weaviate-client` |
| PGVector | pgvector + psycopg2 | `pip install pgvector psycopg2-binary` |
