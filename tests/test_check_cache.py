"""check_cache D 判断测试（批次 D 决策 4：缓存+TTL+有效性+target/version）。"""
import json
import os
import time

from check_cache import check_intel_cache, check_contract_cache, CacheStatus


def _set_age(path, age_hours):
    """设置文件 mtime 为 age_hours 小时前。"""
    ts = time.time() - age_hours * 3600
    os.utime(str(path), (ts, ts))


def _make_intel(root, target="weaviate", age_hours=10, valid=True):
    """造 intelligence/<target>/threat_model.json。"""
    d = root / "intelligence" / target
    d.mkdir(parents=True)
    if valid:
        tm = {"cognitive_blindspots": {"blindspots": ["bs1"]},
              "attack_surface": {"high_priority_areas": []}}
    else:
        tm = {}  # 无效（缺字段）
    p = d / "threat_model.json"
    p.write_text(json.dumps(tm), encoding="utf-8")
    _set_age(p, age_hours)
    return p


def _make_contract(root, target="weaviate", version="1.38.0", age_hours=10, valid=True):
    """造 results/<target>/<version>/structured_contract.json。"""
    d = root / "results" / target / version
    d.mkdir(parents=True)
    if valid:
        c = {"target": target, "version": version,
             "api_endpoints": [{"path": "/v1/objects", "method": "POST",
                                "category": "data", "source_url": "u"}],
             "data_types": [{"name": "vector"}]}
    else:
        c = {"target": target}  # 无效（缺字段）
    p = d / "structured_contract.json"
    p.write_text(json.dumps(c), encoding="utf-8")
    _set_age(p, age_hours)
    return p


def test_intel_cache_fresh_and_valid(tmp_path):
    """情报缓存：存在+未过期+有效 → USABLE。"""
    _make_intel(tmp_path, age_hours=10, valid=True)
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"),
                               "weaviate", ttl_hours=720)
    assert result.status == CacheStatus.USABLE


def test_intel_cache_missing(tmp_path):
    """情报缓存不存在 → MISSING。"""
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate")
    assert result.status == CacheStatus.MISSING


def test_intel_cache_expired(tmp_path):
    """情报缓存过期（age > TTL）→ STALE。"""
    _make_intel(tmp_path, age_hours=1000, valid=True)  # 1000h > 720h TTL
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"),
                               "weaviate", ttl_hours=720)
    assert result.status == CacheStatus.STALE


def test_intel_cache_invalid(tmp_path):
    """情报缓存无效（缺 threat_model 字段）→ INVALID。"""
    _make_intel(tmp_path, age_hours=10, valid=False)
    result = check_intel_cache(str(tmp_path / "intelligence" / "weaviate"), "weaviate")
    assert result.status == CacheStatus.INVALID


def test_contract_cache_fresh_and_valid(tmp_path):
    """契约缓存：存在+未过期+有效+target/version 匹配 → USABLE。"""
    _make_contract(tmp_path, target="weaviate", version="1.38.0", age_hours=10, valid=True)
    result = check_contract_cache(
        str(tmp_path / "results" / "weaviate" / "1.38.0"), "weaviate", "1.38.0", ttl_hours=168)
    assert result.status == CacheStatus.USABLE


def test_contract_cache_target_mismatch(tmp_path):
    """契约缓存 target/version 不匹配 → MISMATCH。"""
    _make_contract(tmp_path, target="qdrant", version="1.18.2")  # 缓存是 qdrant
    result = check_contract_cache(
        str(tmp_path / "results" / "qdrant" / "1.18.2"), "weaviate", "1.38.0")  # 请求 weaviate
    assert result.status == CacheStatus.MISMATCH


def test_contract_cache_invalid(tmp_path):
    """契约缓存无效（缺 api_endpoints）→ INVALID。"""
    _make_contract(tmp_path, valid=False)
    result = check_contract_cache(
        str(tmp_path / "results" / "weaviate" / "1.38.0"), "weaviate", "1.38.0")
    assert result.status == CacheStatus.INVALID
