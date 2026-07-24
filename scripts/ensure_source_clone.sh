#!/usr/bin/env bash
# ensure_source_clone.sh — 确保目标 DB 源码已 clone 到本地，供 dev-reviewer 源码接地（Step 3.5）使用。
#
# 设计：
#   - 共享缓存：${TESTVDB_PLUGIN_ROOT}/.sourcedeps/{target}/{version}/（跨 session 复用，shallow）
#   - 幂等：目录已存在且 HEAD 指向目标 tag → 跳过
#   - 失败（未知 target / tag 不存在 / 网络）→ 非 0 退出，调用方保持 TESTVDB_SRC_DIR 未设，
#     dev-reviewer 自动回退到 WebFetch/curl source_url 的浅核对路径
#
# 输入（环境变量）：
#   TESTVDB_TARGET      e.g. milvus | qdrant | weaviate | chroma | pgvector | meilisearch
#   TESTVDB_VERSION     e.g. v2.6.17（精确 tag，与 Docker 同源）
#   TESTVDB_PLUGIN_ROOT 项目根目录
#
# 输出：
#   成功 → 最后一行打印 `TESTVDB_SRC_DIR=<abs path>`（机器可解析），exit 0
#   失败 → stderr 警告，exit 非 0
#
# 由 orchestrator 在 setup 阶段（Docker 起来后、dev-reviewer 前）调用，
# 拿到 TESTVDB_SRC_DIR 后写入 session 的 .executor.env，与 TESTVDB_DB_URL 同机制被 agent 继承。
set -u

TARGET="${TESTVDB_TARGET:-}"
VERSION="${TESTVDB_VERSION:-}"
ROOT="${TESTVDB_PLUGIN_ROOT:-}"

if [ -z "$TARGET" ] || [ -z "$VERSION" ] || [ -z "$ROOT" ]; then
  echo "[ensure_source_clone] WARN: TESTVDB_TARGET/TESTVDB_VERSION/TESTVDB_PLUGIN_ROOT 未全部设置；跳过源码 clone（dev-reviewer 回退 WebFetch）" >&2
  exit 1
fi

# target -> GitHub repo 映射（Docker image repo 与 GitHub org 可能不同，这里用 GitHub）
case "$TARGET" in
  milvus)      REPO="https://github.com/milvus-io/milvus.git" ;;
  qdrant)      REPO="https://github.com/qdrant/qdrant.git" ;;
  weaviate)    REPO="https://github.com/weaviate/weaviate.git" ;;
  chroma)      REPO="https://github.com/chroma-core/chroma.git" ;;
  pgvector)    REPO="https://github.com/pgvector/pgvector.git" ;;
  meilisearch) REPO="https://github.com/meilisearch/meilisearch.git" ;;
  *)
    echo "[ensure_source_clone] WARN: 未知 target '$TARGET'，无源码 repo 映射（dev-reviewer 回退 WebFetch）" >&2
    exit 1
    ;;
esac

DST="$ROOT/.sourcedeps/$TARGET/$VERSION"

# 幂等：已 clone 且 HEAD 指向目标 tag → 命中缓存
if [ -d "$DST/.git" ]; then
  if git -C "$DST" tag --points-at HEAD 2>/dev/null | grep -qx "$VERSION" \
     || [ "$(git -C "$DST" describe --tags --exact-match HEAD 2>/dev/null)" = "$VERSION" ]; then
    echo "[ensure_source_clone] cache hit: $DST @ $VERSION" >&2
    echo "TESTVDB_SRC_DIR=$DST"
    exit 0
  fi
  # ref 不匹配（stale 或 checkout 到别的 tag）→ 重新 clone
  rm -rf "$DST"
fi

# shallow clone 指定 tag。Docker tag 与 GitHub release tag 的 v 前缀可能不一致 → 失败时重试一次
try_clone() {
  local tag="$1"
  git clone --depth 1 --branch "$tag" "$REPO" "$DST" >&2
}

echo "[ensure_source_clone] cloning $REPO @ $VERSION (shallow) -> $DST" >&2
if try_clone "$VERSION" \
   || { case "$VERSION" in v*) try_clone "${VERSION#v}" ;; *) try_clone "v$VERSION" ;; esac; }; then
  echo "[ensure_source_clone] OK: $DST" >&2
  echo "TESTVDB_SRC_DIR=$DST"
  exit 0
else
  echo "[ensure_source_clone] WARN: clone 失败 ($REPO @ $VERSION；tag 可能不存在于 GitHub 或网络问题)。dev-reviewer 回退 WebFetch。" >&2
  rm -rf "$DST" 2>/dev/null || true
  exit 1
fi
