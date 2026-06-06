#!/usr/bin/env python3
"""
Crawl4AI HTTP Client — 替代 Claude Code 内置 WebFetch 工具。

在 Crawl4AI Docker 容器不可用时自动降级为 requests + html2text。

用法:
    python scripts/crawl_fetch.py <url> [url2 ...]
    python scripts/crawl_fetch.py --raw <url>  # 返回原始 HTML
    python scripts/crawl_fetch.py --json <url>  # 输出 JSON（含元数据）

环境变量:
    CRAWL4AI_BASE_URL    Crawl4AI 服务地址 (默认 http://127.0.0.1:11235)
    CRAWL4AI_API_TOKEN   API 认证 Token（如果 Crawl4AI 设置了）
    CRAWL4AI_TIMEOUT     请求超时秒数 (默认 120)
"""

import json
import os
import sys
import time
import argparse
from urllib.parse import urljoin

# ── HTTP 客户端选择 ──────────────────────────────────────────
try:
    import httpx
    _HTTP_BACKEND = "httpx"
except ImportError:
    import urllib.request
    import urllib.error
    _HTTP_BACKEND = "urllib"


def _http_get(url: str, headers: dict, timeout: int) -> tuple[int, str]:
    """HTTP GET 请求，返回 (status_code, body)。"""
    if _HTTP_BACKEND == "httpx":
        try:
            resp = httpx.get(url, headers=headers, timeout=timeout, follow_redirects=True)
            return resp.status_code, resp.text
        except httpx.HTTPError as e:
            return 0, str(e)
    else:
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.status, resp.read().decode("utf-8", errors="replace")
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode("utf-8", errors="replace")
        except Exception as e:
            return 0, str(e)


def _http_post(url: str, headers: dict, body: dict, timeout: int) -> tuple[int, str]:
    """HTTP POST 请求，返回 (status_code, body)。"""
    if _HTTP_BACKEND == "httpx":
        try:
            resp = httpx.post(url, headers=headers, json=body, timeout=timeout)
            return resp.status_code, resp.text
        except httpx.HTTPError as e:
            return 0, str(e)
    else:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.status, resp.read().decode("utf-8", errors="replace")
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode("utf-8", errors="replace")
        except Exception as e:
            return 0, str(e)


# ── Crawl4AI 客户端 ──────────────────────────────────────────

class Crawl4AIClient:
    """Crawl4AI Docker API 客户端。"""

    def __init__(self, base_url: str, api_token: str | None = None, timeout: int = 120):
        self.base_url = base_url.rstrip("/")
        self.api_token = api_token
        self.timeout = timeout
        self.headers = {"Accept": "application/json"}
        if api_token:
            self.headers["Authorization"] = f"Bearer {api_token}"

    def health(self) -> bool:
        """检查 Crawl4AI 服务是否可用。"""
        url = urljoin(self.base_url + "/", "health")
        code, _ = _http_get(url, self.headers, timeout=10)
        return code == 200

    def crawl(self, urls: list[str]) -> dict | None:
        """提交爬取任务并等待完成，返回结果字典。"""
        # Step 1: 提交任务
        crawl_url = urljoin(self.base_url + "/", "crawl")
        body = {"urls": urls, "priority": 10}
        code, text = _http_post(crawl_url, self.headers, body, timeout=30)

        if code != 200:
            print(f"[crawl4ai] POST /crawl 返回 {code}: {text[:200]}", file=sys.stderr)
            return None

        try:
            task_resp = json.loads(text)
        except json.JSONDecodeError:
            print(f"[crawl4ai] 无法解析响应: {text[:200]}", file=sys.stderr)
            return None

        task_id = task_resp.get("task_id")
        if not task_id:
            print(f"[crawl4ai] 响应中无 task_id: {text[:500]}", file=sys.stderr)
            return None

        # Step 2: 轮询任务状态
        task_url = urljoin(self.base_url + "/", f"task/{task_id}")
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            code, text = _http_get(task_url, self.headers, timeout=30)
            if code != 200:
                print(f"[crawl4ai] GET /task/{task_id} 返回 {code}", file=sys.stderr)
                time.sleep(1)
                continue

            try:
                result = json.loads(text)
            except json.JSONDecodeError:
                print(f"[crawl4ai] 无法解析任务结果: {text[:200]}", file=sys.stderr)
                return None

            status = result.get("status", result.get("state", "unknown"))
            if status in ("completed", "done", "finished"):
                return result
            if status in ("failed", "error"):
                error_msg = result.get("error", result.get("message", "unknown error"))
                print(f"[crawl4ai] 任务失败: {error_msg}", file=sys.stderr)
                return None

            time.sleep(1)

        print(f"[crawl4ai] 任务 {task_id} 超时 ({self.timeout}s)", file=sys.stderr)
        return None

    @staticmethod
    def extract_markdown(result: dict) -> str:
        """从 Crawl4AI 结果中提取 markdown 内容。"""
        # 尝试多种可能的响应格式
        if "markdown" in result:
            return result["markdown"]
        if "result" in result:
            r = result["result"]
            if isinstance(r, dict) and "markdown" in r:
                return r["markdown"]
            if isinstance(r, str):
                return r
        if "content" in result:
            return result["content"]
        if "results" in result and isinstance(result["results"], list):
            parts = []
            for r in result["results"]:
                if isinstance(r, dict):
                    if "markdown" in r:
                        parts.append(r["markdown"])
                    elif "content" in r:
                        parts.append(r["content"])
                elif isinstance(r, str):
                    parts.append(r)
            if parts:
                return "\n\n---\n\n".join(parts)
        # 兜底：返回整个结果的 JSON 字符串
        return json.dumps(result, indent=2, ensure_ascii=False)


# ── 降级方案：requests + html2text ────────────────────────────

def fallback_fetch(url: str, timeout: int = 60) -> str:
    """使用 requests + html2text 作为降级方案。"""
    try:
        import requests
    except ImportError:
        print("[fallback] requests 未安装，尝试 pip install requests", file=sys.stderr)
        return ""

    try:
        import html2text
    except ImportError:
        print("[fallback] html2text 未安装，尝试 pip install html2text", file=sys.stderr)
        return ""

    try:
        resp = requests.get(
            url,
            headers={"User-Agent": "TestVDB-CrawlFetcher/1.0"},
            timeout=timeout,
        )
        resp.raise_for_status()
        converter = html2text.HTML2Text()
        converter.ignore_links = False
        converter.ignore_images = True
        converter.body_width = 0
        return converter.handle(resp.text)
    except Exception as e:
        print(f"[fallback] 抓取失败: {e}", file=sys.stderr)
        return ""


# ── 命令行入口 ────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Crawl4AI HTTP Client — 替代 WebFetch 的网页爬取工具",
    )
    parser.add_argument(
        "urls", nargs="+", help="要抓取的 URL（支持多个）"
    )
    parser.add_argument(
        "--raw", action="store_true", help="返回原始 HTML 而非 Markdown"
    )
    parser.add_argument(
        "--json", action="store_true", help="以 JSON 格式输出（含元数据）"
    )
    parser.add_argument(
        "--fallback", action="store_true", help="强制使用降级方案（requests+html2text）"
    )
    parser.add_argument(
        "--timeout", type=int, default=None, help="请求超时秒数"
    )
    args = parser.parse_args()

    base_url = os.environ.get("CRAWL4AI_BASE_URL", "http://127.0.0.1:11235")
    api_token = os.environ.get("CRAWL4AI_API_TOKEN", "")
    timeout = args.timeout or int(os.environ.get("CRAWL4AI_TIMEOUT", "120"))

    client = Crawl4AIClient(base_url, api_token, timeout)

    # 尝试 Crawl4AI
    if not args.fallback and client.health():
        result = client.crawl(args.urls)
        if result:
            # Crawl4AI 成功
            if args.json:
                print(json.dumps(result, indent=2, ensure_ascii=False))
            elif args.raw:
                print(result.get("html", result.get("cleaned_html", "")))
            else:
                md = client.extract_markdown(result)
                if md:
                    print(md)
                else:
                    print("[crawl_fetch] Crawl4AI 返回了结果但无法提取 markdown", file=sys.stderr)
                    print(json.dumps(result, indent=2, ensure_ascii=False))
            return
        else:
            print("[crawl_fetch] Crawl4AI 抓取失败，尝试降级方案...", file=sys.stderr)
    else:
        if not args.fallback:
            print(f"[crawl_fetch] Crawl4AI 不可达 ({base_url})，使用降级方案...", file=sys.stderr)

    # 降级方案
    for i, url in enumerate(args.urls):
        if i > 0:
            print("\n\n---\n\n")
        content = fallback_fetch(url, timeout)
        if content:
            print(content)
        else:
            print(f"[crawl_fetch] 无法抓取 {url}", file=sys.stderr)
            sys.exit(1)


if __name__ == "__main__":
    main()
