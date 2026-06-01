import argparse
import json
import sys
from collections import Counter


SEVERITY_SCORES = {"P0": 10, "P1": 7, "P2": 4}


def prioritize(issues):
    for issue in issues:
        severity = issue.get("severity", "P2")
        score = SEVERITY_SCORES.get(severity, 4)
        prob = issue.get("acceptance_probability", 0.0)
        issue["priority_score"] = round(score * prob, 4)
    issues.sort(key=lambda x: x["priority_score"], reverse=True)
    return issues


def summarize(issues):
    by_target = Counter(i.get("target", "unknown") for i in issues)
    by_defect_type = Counter(i.get("defect_type", "unknown") for i in issues)
    by_severity = Counter(i.get("severity", "unknown") for i in issues)
    return {
        "total": len(issues),
        "by_target": dict(by_target),
        "by_defect_type": dict(by_defect_type),
        "by_severity": dict(by_severity),
    }


def main():
    parser = argparse.ArgumentParser(description="Prioritize issues by severity x acceptance_probability")
    parser.add_argument("--issues", required=True, help="Path to input JSON file")
    parser.add_argument("--output", help="Path to output JSON file (optional)")
    args = parser.parse_args()

    with open(args.issues, "r", encoding="utf-8") as f:
        issues = json.load(f)

    prioritized = prioritize(issues)
    summary = summarize(prioritized)

    result = {
        "prioritized_issues": prioritized,
        "summary": summary,
    }

    output_json = json.dumps(result, indent=2, ensure_ascii=False)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(output_json)
        print(f"Output written to {args.output}")
    else:
        print(output_json)


if __name__ == "__main__":
    main()
