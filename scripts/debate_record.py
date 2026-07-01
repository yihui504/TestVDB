#!/usr/bin/env python3
"""
DebateRecord — final_verdict.json schema owner (ADR-0005).

Provides typed access to the sole source of truth for defect verdicts.
Consumers (reporter, reporter-mre, reconstruct_context) import this
module instead of reading raw JSON with defensive .get() calls.

Usage:
  from debate_record import FinalVerdict
  v = FinalVerdict.from_file(session_dir)
  for d in v.endorsed():
      print(d.defect_id)
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


# ── Exceptions ────────────────────────────────────────────────

class SchemaValidationError(ValueError):
    """Raised when final_verdict.json fails schema validation."""
    def __init__(self, missing_field: str, expected_type: str = ""):
        msg = f"Missing required field: {missing_field}"
        if expected_type:
            msg += f" (expected {expected_type})"
        super().__init__(msg)


# ── Data classes ──────────────────────────────────────────────

@dataclass
class DefectVerdict:
    """A single defect's full verdict from the Novelty Gate + 4-Judge pipeline."""

    defect_id: str
    script: str
    param: str
    param_name: str
    defect_type: str
    judge_doc: str
    judge_evidence: str
    judge_novelty: str
    judge_severity: str
    gate_grade: str
    gate_layer: str
    gate_evidence_url: str
    endorsement: bool
    endorsement_reason: str
    judge_discrepancy: bool

    @classmethod
    def from_dict(cls, d: dict) -> "DefectVerdict":
        """Construct from a raw dict with validation."""
        required = [
            "defect_id", "script", "param", "param_name", "defect_type",
            "judge_doc", "judge_evidence", "judge_novelty", "judge_severity",
            "gate_grade", "gate_layer", "gate_evidence_url",
            "endorsement", "endorsement_reason", "judge_discrepancy",
        ]
        for field in required:
            if field not in d:
                raise SchemaValidationError(field)
        return cls(**{f: d[f] for f in required})


@dataclass
class FinalVerdict:
    """The authoritative final_verdict.json (ADR-0002)."""

    generated_at: str
    session_dir: str
    total_defects: int
    defects: list[DefectVerdict]

    @classmethod
    def from_file(cls, session_dir: str | Path) -> "FinalVerdict":
        """Load and validate final_verdict.json from a session directory.

        Raises SchemaValidationError if the file is missing required fields.
        Raises FileNotFoundError if the file does not exist.
        """
        sd = Path(session_dir)
        path = sd / "debate_logs" / "final_verdict.json"
        if not path.exists():
            raise FileNotFoundError(str(path))

        with open(path, encoding="utf-8") as f:
            data = json.load(f)

        # Validate top-level fields
        for field in ("generated_at", "session_dir", "total_defects", "defects"):
            if field not in data:
                raise SchemaValidationError(field)

        defects = [DefectVerdict.from_dict(d) for d in data.get("defects", [])]
        return cls(
            generated_at=data["generated_at"],
            session_dir=data["session_dir"],
            total_defects=data["total_defects"],
            defects=defects,
        )

    def endorsed(self) -> list[DefectVerdict]:
        """Return defects with endorsement == True (Gate-Endorsed)."""
        return [d for d in self.defects if d.endorsement]

    def rejected(self) -> list[DefectVerdict]:
        """Return defects with endorsement == False."""
        return [d for d in self.defects if not d.endorsement]

    def summary(self) -> dict:
        """Return a stable summary dict for consumers."""
        grades: dict[str, int] = {}
        for d in self.defects:
            g = d.gate_grade
            grades[g] = grades.get(g, 0) + 1
        return {
            "total": self.total_defects,
            "endorsed": len(self.endorsed()),
            "rejected": len(self.rejected()),
            "grades": grades,
            "generated_at": self.generated_at,
        }

    def by_grade(self, grade: str) -> list[DefectVerdict]:
        """Filter defects by gate_grade."""
        return [d for d in self.defects if d.gate_grade == grade]

    def discrepancies(self) -> list[DefectVerdict]:
        """Return defects where Triage disagreed with Gate."""
        return [d for d in self.defects if d.judge_discrepancy]
