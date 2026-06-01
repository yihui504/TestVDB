import argparse
import json
import os
import re
import hashlib
import sys

import requests

CACHE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".cache", "github_attitude")


def _cache_path(owner, repo, number, suffix):
    key = f"{owner}/{repo}/{number}/{suffix}"
    h = hashlib.sha256(key.encode()).hexdigest()
    return os.path.join(CACHE_DIR, h + ".json")


def _load_cache(path):
    if os.path.isfile(path):
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    return None


def _save_cache(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False)


def _parse_issue_url(url):
    m = re.match(r"https?://github\.com/([^/]+)/([^/]+)/issues/(\d+)", url)
    if not m:
        print(json.dumps({"error": f"Invalid issue URL: {url}"}), file=sys.stderr)
        sys.exit(1)
    return m.group(1), m.group(2), int(m.group(3))


def _github_get(url, token, params=None):
    headers = {"Accept": "application/vnd.github+json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    resp = requests.get(url, headers=headers, params=params, timeout=30)
    resp.raise_for_status()
    return resp.json()


def _fetch_issue(owner, repo, number, token):
    cache = _cache_path(owner, repo, number, "issue")
    if token:
        try:
            url = f"https://api.github.com/repos/{owner}/{repo}/issues/{number}"
            data = _github_get(url, token)
            _save_cache(cache, data)
            return data
        except Exception:
            cached = _load_cache(cache)
            if cached is not None:
                return cached
            return None
    cached = _load_cache(cache)
    if cached is not None:
        return cached
    return None


def _fetch_comments(owner, repo, number, token):
    cache = _cache_path(owner, repo, number, "comments")
    if token:
        try:
            url = f"https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments"
            data = _github_get(url, token)
            _save_cache(cache, data)
            return data
        except Exception:
            cached = _load_cache(cache)
            if cached is not None:
                return cached
            return None
    cached = _load_cache(cache)
    if cached is not None:
        return cached
    return None


def _fetch_collaborators(owner, repo, token):
    cache = _cache_path(owner, repo, 0, "collaborators")
    if token:
        url = f"https://api.github.com/repos/{owner}/{repo}/collaborators"
        try:
            data = _github_get(url, token, params={"affiliation": "direct"})
            _save_cache(cache, data)
            return data
        except Exception:
            cached = _load_cache(cache)
            if cached is not None:
                return cached
            return []
    cached = _load_cache(cache)
    if cached is not None:
        return cached
    return None


def _extract_signals(issue, comments, collaborators, owner):
    signals = []
    label_names = [lbl.get("name", "") for lbl in issue.get("labels", [])]

    if "triage/accepted" in label_names:
        signals.append({"signal": "ACCEPTED", "source": "label:triage/accepted"})

    if "kind/bug" in label_names:
        signals.append({"signal": "CONFIRMED_BUG", "source": "label:kind/bug"})

    if issue.get("assignee") or issue.get("assignees"):
        assignees = []
        if issue.get("assignee"):
            assignees.append(issue["assignee"].get("login", ""))
        for a in issue.get("assignees", []):
            assignees.append(a.get("login", ""))
        signals.append({"signal": "CLAIMED", "source": f"assignee:{','.join(assignees)}"})

    if issue.get("milestone"):
        signals.append({"signal": "SCHEDULED", "source": f"milestone:{issue['milestone'].get('title', '')}"})

    state_reason = issue.get("state_reason")
    if issue.get("state") == "closed":
        if state_reason == "not_planned":
            signals.append({"signal": "REJECTED", "source": "closed_as:not_planned"})
        elif state_reason == "completed":
            signals.append({"signal": "FIXED", "source": "closed_as:completed"})

    core_logins = set()
    if collaborators:
        for c in collaborators:
            core_logins.add(c.get("login", ""))
    core_logins.add(owner)

    if comments:
        for c in comments:
            user = c.get("user", {}).get("login", "")
            if user in core_logins:
                signals.append({"signal": "CORE_DEV_COMMENTED", "source": f"commenter:{user}"})
                break

    return signals


def main():
    parser = argparse.ArgumentParser(description="Extract developer attitude signals from a GitHub Issue")
    parser.add_argument("--issue-url", required=True, help="GitHub Issue URL")
    parser.add_argument("--github-token", default=os.environ.get("GITHUB_TOKEN"), help="GitHub personal access token")
    args = parser.parse_args()

    owner, repo, number = _parse_issue_url(args.issue_url)
    token = args.github_token
    warnings = []

    issue = _fetch_issue(owner, repo, number, token)
    if issue is None:
        warnings.append(f"WARNING: No token and no cache for issue {owner}/{repo}#{number}, skipping issue fetch")

    comments = _fetch_comments(owner, repo, number, token)
    if comments is None:
        warnings.append(f"WARNING: No token and no cache for comments on {owner}/{repo}#{number}, skipping comments fetch")

    collaborators = _fetch_collaborators(owner, repo, token)
    if collaborators is None:
        warnings.append(f"WARNING: No token and no cache for collaborators on {owner}/{repo}, core dev detection may be incomplete")

    if issue is None:
        result = {"issue_url": args.issue_url, "signals": [], "warnings": warnings}
        print(json.dumps(result, indent=2, ensure_ascii=False))
        sys.exit(0)

    signals = _extract_signals(issue, comments or [], collaborators or [], owner)

    result = {"issue_url": args.issue_url, "signals": signals}
    if warnings:
        result["warnings"] = warnings

    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
