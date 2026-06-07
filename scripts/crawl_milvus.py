#!/usr/bin/env python3
"""Milvus 2.6.17 documentation crawler using Crawl4AI v0.8.9."""
import httpx
import json
import sys
import os
import time

CRAWL4AI_URL = os.environ.get("CRAWL4AI_BASE_URL", "http://127.0.0.1:11235")
OUTPUT_DIR = os.environ.get("OUTPUT_DIR", "results/milvus/2.6.17")

MILVUS_URLS = [
    # RESTful API reference
    ("restful_about", "https://milvus.io/api-reference/restful/v2.6.x/About.md"),
    ("restful_collection_create", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Create.md"),
    ("restful_collection_drop", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Drop.md"),
    ("restful_collection_describe", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Describe.md"),
    ("restful_collection_list", "https://milvus.io/api-reference/restful/v2.6.x/Collection/List.md"),
    ("restful_collection_load", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Load.md"),
    ("restful_collection_release", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Release.md"),
    ("restful_collection_rename", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Rename.md"),
    ("restful_collection_get_stats", "https://milvus.io/api-reference/restful/v2.6.x/Collection/Get_Collection_Stats.md"),
    ("restful_partition_create", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Create.md"),
    ("restful_partition_drop", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Drop.md"),
    ("restful_partition_list", "https://milvus.io/api-reference/restful/v2.6.x/Partition/List.md"),
    ("restful_partition_describe", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Describe.md"),
    ("restful_partition_load", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Load.md"),
    ("restful_partition_release", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Release.md"),
    ("restful_partition_has", "https://milvus.io/api-reference/restful/v2.6.x/Partition/Has.md"),
    ("restful_vector_insert", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Insert.md"),
    ("restful_vector_upsert", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Upsert.md"),
    ("restful_vector_delete", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Delete.md"),
    ("restful_vector_search", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Search.md"),
    ("restful_vector_query", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Query.md"),
    ("restful_vector_get", "https://milvus.io/api-reference/restful/v2.6.x/Vector/Get.md"),
    ("restful_index_create", "https://milvus.io/api-reference/restful/v2.6.x/Index/Create.md"),
    ("restful_index_drop", "https://milvus.io/api-reference/restful/v2.6.x/Index/Drop.md"),
    ("restful_index_describe", "https://milvus.io/api-reference/restful/v2.6.x/Index/Describe.md"),
    ("restful_index_list", "https://milvus.io/api-reference/restful/v2.6.x/Index/List.md"),
    ("restful_alias_create", "https://milvus.io/api-reference/restful/v2.6.x/Alias/Create.md"),
    ("restful_alias_drop", "https://milvus.io/api-reference/restful/v2.6.x/Alias/Drop.md"),
    ("restful_alias_describe", "https://milvus.io/api-reference/restful/v2.6.x/Alias/Describe.md"),
    ("restful_alias_list", "https://milvus.io/api-reference/restful/v2.6.x/Alias/List.md"),
    ("restful_user_create", "https://milvus.io/api-reference/restful/v2.6.x/User/Create.md"),
    ("restful_user_drop", "https://milvus.io/api-reference/restful/v2.6.x/User/Drop.md"),
    ("restful_user_update", "https://milvus.io/api-reference/restful/v2.6.x/User/Update.md"),
    ("restful_user_list", "https://milvus.io/api-reference/restful/v2.6.x/User/List.md"),
    ("restful_user_describe", "https://milvus.io/api-reference/restful/v2.6.x/User/Describe.md"),
    ("restful_role_create", "https://milvus.io/api-reference/restful/v2.6.x/Role/Create.md"),
    ("restful_role_drop", "https://milvus.io/api-reference/restful/v2.6.x/Role/Drop.md"),
    ("restful_role_describe", "https://milvus.io/api-reference/restful/v2.6.x/Role/Describe.md"),
    ("restful_role_list", "https://milvus.io/api-reference/restful/v2.6.x/Role/List.md"),
    ("restful_import", "https://milvus.io/api-reference/restful/v2.6.x/Import/Import.md"),
    ("restful_import_state", "https://milvus.io/api-reference/restful/v2.6.x/Import/Get_Import_State.md"),
    ("restful_import_list", "https://milvus.io/api-reference/restful/v2.6.x/Import/List_Imports.md"),
    # PyMilvus SDK reference
    ("pymilvus_about", "https://milvus.io/api-reference/pymilvus/v2.6.x/About.md"),
    ("pymilvus_collection", "https://milvus.io/api-reference/pymilvus/v2.6.x/Collection/Collection.md"),
    ("pymilvus_utility", "https://milvus.io/api-reference/pymilvus/v2.6.x/About.md"),
    # Main docs
    ("milvus_docs_home", "https://milvus.io/docs/overview.md"),
]


def crawl_url(name: str, url: str) -> str | None:
    print(f"[crawl] Fetching {name}: {url}...", file=sys.stderr)
    try:
        resp = httpx.post(
            f"{CRAWL4AI_URL}/crawl",
            json={"urls": [url]},
            timeout=120,
        )
        if resp.status_code != 200:
            print(f"[crawl] Error {resp.status_code} for {name}", file=sys.stderr)
            return None
        data = resp.json()
        results = data.get("results", [])
        if not results:
            print(f"[crawl] No results for {name}: {json.dumps(data)[:300]}", file=sys.stderr)
            return None
        r = results[0]
        md = r.get("markdown", {})
        if isinstance(md, dict):
            content = md.get("raw_markdown", md.get("fit_markdown", ""))
        elif isinstance(md, str):
            content = md
        else:
            content = ""
        if content:
            print(f"[crawl] Got {len(content)} chars from {name}", file=sys.stderr)
            return content
        # fallback: use fit_html or cleaned_html
        html = r.get("fit_html", r.get("cleaned_html", ""))
        if html:
            print(f"[crawl] Got HTML ({len(html)} chars) from {name}", file=sys.stderr)
            return html
        print(f"[crawl] Empty content for {name}", file=sys.stderr)
        return None
    except Exception as e:
        print(f"[crawl] Exception for {name}: {e}", file=sys.stderr)
        return None


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    all_content = []
    for i, (name, url) in enumerate(MILVUS_URLS):
        content = crawl_url(name, url)
        if content:
            filepath = os.path.join(OUTPUT_DIR, f"doc_{name}.md")
            with open(filepath, "w", encoding="utf-8") as f:
                f.write(content)
            print(f"[crawl] Saved to {filepath}", file=sys.stderr)
            all_content.append(f"## Source: {name}\nURL: {url}\n\n{content[:500]}...\n\n")
        else:
            print(f"[crawl] FAILED: {name}", file=sys.stderr)
        # Small delay between requests
        if i < len(MILVUS_URLS) - 1:
            time.sleep(0.5)

    # Write a summary
    summary_path = os.path.join(OUTPUT_DIR, "crawl_summary.json")
    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump({
            "total_urls": len(MILVUS_URLS),
            "successful": sum(1 for n, u in MILVUS_URLS
                             if os.path.exists(os.path.join(OUTPUT_DIR, f"doc_{n}.md"))),
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        }, f, indent=2)
    print(f"[crawl] Summary saved to {summary_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
