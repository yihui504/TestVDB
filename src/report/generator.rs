use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::agent::classifier::DefectType;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunEvidence {
    pub phase: String,
    pub db_url: String,
    pub stdout: String,
    pub stderr: String,
    pub classifier_reason: String,
    pub classifier_evidence_excerpt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum CandidateStatus {
    Pending,
    ReproducedTwice,
    Rejected,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CandidateDefect {
    pub target: String,
    pub version: String,
    pub defect_type: DefectType,
    pub doc_citation_url: String,
    pub contract_assertions: Vec<String>,
    pub surviving_assertions: Vec<String>,
    pub mre_code: String,
    pub initial_run: RunEvidence,
    pub reproduction_runs: Vec<RunEvidence>,
    pub status: CandidateStatus,
    pub downgrade_reason: Option<String>,
    pub independent_review_summary: Option<String>,
    pub review_scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum SubmissionGradeVerdict {
    SubmissionGrade,
    NeedsRewrite,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SubmissionGradeGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SubmissionGradeReview {
    pub verdict: SubmissionGradeVerdict,
    pub summary: String,
    pub hard_gates: Vec<SubmissionGradeGate>,
    pub soft_gates: Vec<SubmissionGradeGate>,
    pub direct_fail_reasons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BugReport {
    pub title: String,
    pub target: String,
    pub version: String,
    pub defect_type: DefectType,
    pub doc_citation_url: String,
    pub contract_assertions: Vec<String>,
    pub surviving_assertions: Vec<String>,
    pub mre_code: String,
    pub rerun_instructions: String,
    pub initial_run_summary: String,
    pub reproduction_summary: String,
    pub classification_basis: String,
    pub runtime_evidence: String,
    pub independent_review_summary: String,
    pub review_scope: String,
    pub root_cause_analysis: String,
    pub improvement_suggestions: String,
    pub submission_grade_review: SubmissionGradeReview,
}

impl BugReport {
    fn stabilize_mre_code(mre_code: &str, db_url: &str) -> String {
        if db_url.trim().is_empty() {
            return mre_code.to_string();
        }
        mre_code.replace(db_url, "{{TESTVDB_DB_URL}}")
    }

    fn default_independent_review_summary() -> String {
        "Independent developer-side review was not recorded for this report path, so submission-grade review cannot pass yet.".to_string()
    }

    fn default_review_scope() -> String {
        "Independent review scope was not recorded for this report path.".to_string()
    }

    fn evaluate_submission_grade(&self) -> SubmissionGradeReview {
        let doc_gate = SubmissionGradeGate {
            name: "Documentation and contract binding".to_string(),
            passed: self.doc_citation_url.starts_with("http")
                && !self.contract_assertions.is_empty()
                && !self.surviving_assertions.is_empty(),
            detail: format!(
                "Source URL present: {}; original contract assertions: {}; surviving assertions under report: {}.",
                self.doc_citation_url.starts_with("http"),
                self.contract_assertions.len(),
                self.surviving_assertions.len()
            ),
        };

        let mre_gate = SubmissionGradeGate {
            name: "MRE and rerun evidence".to_string(),
            passed: self.mre_code.contains("{{TESTVDB_DB_URL}}")
                && !self.rerun_instructions.trim().is_empty()
                && self.runtime_evidence.contains("Initial Evidence Excerpt:")
                && self.runtime_evidence.contains("Reproductions:"),
            detail: format!(
                "MRE placeholder present: {}; runtime evidence includes replay summary: {}.",
                self.mre_code.contains("{{TESTVDB_DB_URL}}"),
                self.runtime_evidence.contains("Reproductions:")
            ),
        };

        let independent_review_recorded = !self.independent_review_summary.trim().is_empty()
            && self.independent_review_summary != Self::default_independent_review_summary();
        let reproduction_gate = SubmissionGradeGate {
            name: "Double reproduction and independent review".to_string(),
            passed: self.reproduction_summary.contains("repro_1")
                && self.reproduction_summary.contains("repro_2")
                && !self.classification_basis.trim().is_empty()
                && independent_review_recorded
                && self.review_scope != Self::default_review_scope(),
            detail: format!(
                "Double reproduction recorded: {}; independent review recorded: {}.",
                self.reproduction_summary.contains("repro_1") && self.reproduction_summary.contains("repro_2"),
                independent_review_recorded
            ),
        };

        let readability_gate = SubmissionGradeGate {
            name: "Report readability".to_string(),
            passed: !self.root_cause_analysis.trim().is_empty()
                && !self.improvement_suggestions.trim().is_empty()
                && !self.review_scope.trim().is_empty(),
            detail: "Root cause, improvement suggestions, and review scope are all present.".to_string(),
        };

        let hard_gates = vec![doc_gate, mre_gate, reproduction_gate];
        let soft_gates = vec![readability_gate];
        let mut direct_fail_reasons = hard_gates
            .iter()
            .filter(|gate| !gate.passed)
            .map(|gate| format!("Missing hard gate: {}.", gate.name))
            .collect::<Vec<_>>();

        if self.classification_basis.trim().is_empty()
            || self.initial_run_summary.trim().is_empty()
            || self.reproduction_summary.trim().is_empty()
        {
            direct_fail_reasons.push(
                "Core conclusion or verification summary is incomplete and still needs major rewriting.".to_string(),
            );
        }

        let verdict = if direct_fail_reasons.is_empty() {
            SubmissionGradeVerdict::SubmissionGrade
        } else {
            SubmissionGradeVerdict::NeedsRewrite
        };
        let summary = match verdict {
            SubmissionGradeVerdict::SubmissionGrade => {
                "All hard gates are present; the report is submission-grade under the current Phase 5 rubric.".to_string()
            }
            SubmissionGradeVerdict::NeedsRewrite => {
                "The report is not submission-grade yet because at least one hard gate or direct-fail condition is still open.".to_string()
            }
        };

        SubmissionGradeReview {
            verdict,
            summary,
            hard_gates,
            soft_gates,
            direct_fail_reasons,
        }
    }

    pub fn from_verified_candidate(candidate: &CandidateDefect) -> Result<Self> {
        if candidate.status != CandidateStatus::ReproducedTwice {
            bail!("Candidate defect has not passed double reproduction.");
        }

        let reproduction_summary = candidate
            .reproduction_runs
            .iter()
            .map(|run| format!("{}: {}", run.phase, run.classifier_reason))
            .collect::<Vec<_>>()
            .join("; ");

        let independent_review_summary = candidate
            .independent_review_summary
            .clone()
            .unwrap_or_else(Self::default_independent_review_summary);
        let review_scope = candidate
            .review_scope
            .clone()
            .unwrap_or_else(Self::default_review_scope);

        let root_cause_analysis = match candidate.defect_type {
            DefectType::PoorDiagnostics => format!(
                "Across the initial run, two fresh sandbox reproductions, and the independent replay scope, the server rejected the invalid request but did not clearly explain these violated constraints: {}.",
                candidate.surviving_assertions.join("; ")
            ),
            DefectType::IllegalSuccess => {
                let issues = candidate.surviving_assertions.join("; ");
                format!(
                    "The database accepted an operation that the cited contract treats as invalid ({}). This indicates that the server-side request validation is either missing or too permissive for the affected parameter(s), allowing the operation to proceed to a success response instead of being rejected at the boundary.",
                    issues
                )
            }
            DefectType::RuntimeFailure => {
                "The target reached the execution path under test and then failed at runtime, which points to a server-side stability or error-handling defect.".to_string()
            }
            DefectType::StateLogicViolation => {
                "Observed state diverged from the documented contract after the operation completed, which suggests the server-side state transition or result assembly is inconsistent.".to_string()
            }
            DefectType::DataCorruption => {
                "Data written to the database differs from data read back, which suggests the server-side storage or serialization is corrupting data silently.".to_string()
            }
            DefectType::PerformanceRegression => {
                "An operation took significantly longer than the expected baseline, which suggests a performance regression in the server-side processing path.".to_string()
            }
            DefectType::MetamorphicViolation => {
                "A metamorphic relation that should hold between related operations was violated, indicating a semantic inconsistency in the server's behavior.".to_string()
            }
            DefectType::DifferentialMismatch => {
                "The same operation produced different results through different API paths (e.g., REST vs SDK), indicating an inconsistency in the server's implementation.".to_string()
            }
            DefectType::SequenceViolation => {
                "An API call sequence produced an unexpected state or result, indicating a state transition or ordering dependency that violates documented behavior.".to_string()
            }
            DefectType::ScriptError | DefectType::Pass => {
                "This report should not have been promoted for the observed classification.".to_string()
            }
        };

        let improvement_suggestions = match candidate.defect_type {
            DefectType::PoorDiagnostics => format!(
                "Update the error response so it explicitly mentions the violated constraint(s): {}.",
                candidate.surviving_assertions.join("; ")
            ),
            DefectType::IllegalSuccess => {
                let issues = candidate.surviving_assertions.join("; ");
                format!(
                    "Add or tighten request validation for the affected parameter(s) so the invalid operation is rejected at the boundary before success is returned. Specifically: {}. After the fix, add a regression test that asserts the documented constraint produces a 400/422 rejection rather than 200 OK.",
                    issues
                )
            }
            DefectType::RuntimeFailure => {
                "Harden the execution path that handles this request and add regression coverage for the failing scenario.".to_string()
            }
            DefectType::StateLogicViolation => {
                "Audit the state transition and response-building path for this scenario, then add a regression test that asserts the documented invariant.".to_string()
            }
            DefectType::DataCorruption => {
                "Audit the write and read paths for data serialization and storage consistency, then add a regression test that writes data and verifies the read-back matches.".to_string()
            }
            DefectType::PerformanceRegression => {
                "Profile the slow operation path to identify the bottleneck, then add a performance regression test with a timing assertion.".to_string()
            }
            DefectType::MetamorphicViolation => {
                "Audit the metamorphic relation that was violated and add a regression test that asserts the relation holds for the affected operation pair.".to_string()
            }
            DefectType::DifferentialMismatch => {
                "Ensure the REST and SDK paths use the same underlying implementation for the affected operation, then add a regression test that asserts both paths return consistent results.".to_string()
            }
            DefectType::SequenceViolation => {
                "Audit the state transition logic for the affected API sequence and add a regression test that asserts the documented ordering constraints hold.".to_string()
            }
            DefectType::ScriptError | DefectType::Pass => {
                "Do not promote this result to a formal defect report until the classification path is corrected.".to_string()
            }
        };

        let mut report = Self {
            title: format!(
                "Verified {:?}: {} {}",
                candidate.defect_type, candidate.target, candidate.version
            ),
            target: candidate.target.clone(),
            version: candidate.version.clone(),
            defect_type: candidate.defect_type.clone(),
            doc_citation_url: candidate.doc_citation_url.clone(),
            contract_assertions: candidate.contract_assertions.clone(),
            surviving_assertions: candidate.surviving_assertions.clone(),
            mre_code: Self::stabilize_mre_code(&candidate.mre_code, &candidate.initial_run.db_url),
            rerun_instructions: "Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.".to_string(),
            initial_run_summary: candidate.initial_run.classifier_reason.clone(),
            reproduction_summary,
            classification_basis: format!(
                "Initial run and {} fresh-sandbox reproductions produced consistent {:?} classification and matching evidence excerpts.",
                candidate.reproduction_runs.len(),
                candidate.defect_type
            ),
            runtime_evidence: format!(
                "Initial DB URL: {}\nInitial Evidence Excerpt: {}\n\nInitial STDOUT:\n{}\n\nInitial STDERR:\n{}\n\nReproductions:\n{}",
                candidate.initial_run.db_url,
                candidate.initial_run.classifier_evidence_excerpt,
                candidate.initial_run.stdout,
                candidate.initial_run.stderr,
                candidate
                    .reproduction_runs
                    .iter()
                    .map(|run| format!(
                        "{}\nDB URL: {}\nReason: {}\nEvidence Excerpt: {}\nSTDOUT:\n{}\nSTDERR:\n{}\n",
                        run.phase,
                        run.db_url,
                        run.classifier_reason,
                        run.classifier_evidence_excerpt,
                        run.stdout,
                        run.stderr
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            independent_review_summary,
            review_scope,
            root_cause_analysis,
            improvement_suggestions,
            submission_grade_review: SubmissionGradeReview {
                verdict: SubmissionGradeVerdict::NeedsRewrite,
                summary: String::new(),
                hard_gates: Vec::new(),
                soft_gates: Vec::new(),
                direct_fail_reasons: Vec::new(),
            },
        };
        report.submission_grade_review = report.evaluate_submission_grade();

        Ok(report)
    }

    /// Validates if the bug report meets the strict Acceptance Criteria
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("Bug report is missing a title.");
        }
        if self.target.trim().is_empty() || self.version.trim().is_empty() {
            bail!("Bug report must include target and version.");
        }
        if self.doc_citation_url.trim().is_empty() || !self.doc_citation_url.starts_with("http") {
            bail!("Bug report must include a valid official document citation URL.");
        }
        if self.contract_assertions.is_empty() {
            bail!("Bug report must include contract assertions.");
        }
        if self.surviving_assertions.is_empty() {
            bail!("Bug report must include surviving assertions under report.");
        }
        if self.mre_code.trim().is_empty() {
            bail!("Bug report must include Minimal Reproducible Example (MRE) code.");
        }
        if self.rerun_instructions.trim().is_empty() {
            bail!("Bug report must include rerun instructions.");
        }
        if self.initial_run_summary.trim().is_empty()
            || self.reproduction_summary.trim().is_empty()
            || self.classification_basis.trim().is_empty()
        {
            bail!("Bug report must include run and reproduction summaries.");
        }
        if self.runtime_evidence.trim().is_empty() {
            bail!("Bug report must include runtime evidence.");
        }
        if self.independent_review_summary.trim().is_empty() {
            bail!("Bug report must include independent review summary.");
        }
        if self.review_scope.trim().is_empty() {
            bail!("Bug report must include review scope.");
        }
        if self.root_cause_analysis.trim().is_empty() {
            bail!("Bug report must include a root cause analysis.");
        }
        if self.improvement_suggestions.trim().is_empty() {
            bail!("Bug report must include improvement suggestions.");
        }
        if self.submission_grade_review.summary.trim().is_empty()
            || self.submission_grade_review.hard_gates.is_empty()
        {
            bail!("Bug report must include a submission-grade review result.");
        }
        Ok(())
    }

    /// Exports the bug report to a Markdown file
    pub fn export_to_markdown<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let assertions = self
            .contract_assertions
            .iter()
            .map(|assertion| format!("- {}", assertion))
            .collect::<Vec<_>>()
            .join("\n");
        let surviving_assertions = self
            .surviving_assertions
            .iter()
            .map(|assertion| format!("- {}", assertion))
            .collect::<Vec<_>>()
            .join("\n");
        let hard_gates = self
            .submission_grade_review
            .hard_gates
            .iter()
            .map(|gate| {
                format!(
                    "- [{}] {}: {}",
                    if gate.passed { "PASS" } else { "FAIL" },
                    gate.name,
                    gate.detail
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let soft_gates = self
            .submission_grade_review
            .soft_gates
            .iter()
            .map(|gate| {
                format!(
                    "- [{}] {}: {}",
                    if gate.passed { "PASS" } else { "FAIL" },
                    gate.name,
                    gate.detail
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let direct_fail_reasons = if self.submission_grade_review.direct_fail_reasons.is_empty() {
            "- None".to_string()
        } else {
            self.submission_grade_review
                .direct_fail_reasons
                .iter()
                .map(|reason| format!("- {}", reason))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let verdict = match self.submission_grade_review.verdict {
            SubmissionGradeVerdict::SubmissionGrade => "SubmissionGrade",
            SubmissionGradeVerdict::NeedsRewrite => "NeedsRewrite",
        };

        let markdown = format!(
            "# {}\n\n\
            - **Target**: {}\n\
            - **Version**: {}\n\
            - **Defect Type**: {:?}\n\n\
            ## Documentation Evidence\n\
            - **Source URL**: {}\n\
            - **Documented Contract Assertions**:\n{}\n\
            - **Surviving Assertions Under Report**:\n{}\n\n\
            ## Minimal Reproducible Example (MRE)\n\
            ```\n\
            {}\n\
            ```\n\n\
            ## Rerun Instructions\n\
            {}\n\n\
            ## Verification Summary\n\
            - **Initial Run**: {}\n\
            - **Double Reproduction**: {}\n\
            - **Classification Basis**: {}\n\n\
            ## Runtime Evidence\n\
            ```\n\
            {}\n\
            ```\n\n\
            ## Independent Review\n\
            - **Summary**: {}\n\
            - **Scope**: {}\n\n\
            ## Submission-Grade Review\n\
            - **Verdict**: {}\n\
            - **Summary**: {}\n\
            - **Hard Gates**:\n{}\n\
            - **Soft Gates**:\n{}\n\
            - **Direct-Fail Reasons**:\n{}\n\n\
            ## Root Cause Analysis\n\
            {}\n\n\
            ## Improvement Suggestions\n\
            {}\n",
            self.title,
            self.target,
            self.version,
            self.defect_type,
            self.doc_citation_url,
            assertions,
            surviving_assertions,
            self.mre_code,
            self.rerun_instructions,
            self.initial_run_summary,
            self.reproduction_summary,
            self.classification_basis,
            self.runtime_evidence,
            self.independent_review_summary,
            self.review_scope,
            verdict,
            self.submission_grade_review.summary,
            hard_gates,
            soft_gates,
            direct_fail_reasons,
            self.root_cause_analysis,
            self.improvement_suggestions
        );

        fs::write(path, markdown).context("Failed to write Markdown bug report to file")?;
        Ok(())
    }

    pub fn export_candidate_to_markdown<P: AsRef<Path>>(
        candidate: &CandidateDefect,
        path: P,
    ) -> Result<()> {
        let assertions = candidate
            .contract_assertions
            .iter()
            .map(|assertion| format!("- {}", assertion))
            .collect::<Vec<_>>()
            .join("\n");

        let reproduction_summary = if candidate.reproduction_runs.is_empty() {
            "No fresh-sandbox reproductions completed.".to_string()
        } else {
            candidate
                .reproduction_runs
                .iter()
                .map(|run| format!("- {}: {}", run.phase, run.classifier_reason))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let markdown = format!(
            "# Candidate Defect: {:?}\n\n\
            - **Target**: {}\n\
            - **Version**: {}\n\
            - **Status**: {:?}\n\
            - **Downgrade Reason**: {}\n\n\
            ## Documentation Evidence\n\
            - **Source URL**: {}\n\
            - **Contract Assertions**:\n{}\n\n\
            ## MRE\n\
            ```\n{}\n```\n\n\
            ## Initial Run\n\
            - **Reason**: {}\n\
            - **DB URL**: {}\n\n\
            - **Evidence Excerpt**: {}\n\n\
            ### STDOUT\n\
            ```\n{}\n```\n\n\
            ### STDERR\n\
            ```\n{}\n```\n\n\
            ## Reproduction Attempts\n\
            {}\n",
            candidate.defect_type,
            candidate.target,
            candidate.version,
            candidate.status,
            candidate
                .downgrade_reason
                .clone()
                .unwrap_or_else(|| "None".to_string()),
            candidate.doc_citation_url,
            assertions,
            Self::stabilize_mre_code(&candidate.mre_code, &candidate.initial_run.db_url),
            candidate.initial_run.classifier_reason,
            candidate.initial_run.db_url,
            candidate.initial_run.classifier_evidence_excerpt,
            candidate.initial_run.stdout,
            candidate.initial_run.stderr,
            reproduction_summary
        );

        fs::write(path, markdown).context("Failed to write candidate defect report to file")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bug_report_validation_success() {
        let report = BugReport {
            title: "Dimension mismatch issue".to_string(),
            target: "milvus".to_string(),
            version: "2.0.0".to_string(),
            defect_type: DefectType::PoorDiagnostics,
            doc_citation_url: "https://milvus.io/docs/create.md".to_string(),
            contract_assertions: vec!["Dimension must be greater than 0".to_string()],
            surviving_assertions: vec!["Dimension must be greater than 0".to_string()],
            mre_code: "client.create_collection(dimension=-1)".to_string(),
            rerun_instructions: "Replace placeholder".to_string(),
            initial_run_summary: "Initial run classified as PoorDiagnostics".to_string(),
            reproduction_summary: "repro_1 and repro_2 matched".to_string(),
            classification_basis: "Three consistent classifications".to_string(),
            runtime_evidence: "stdout/stderr".to_string(),
            independent_review_summary: "Independent replay confirmed the same issue.".to_string(),
            review_scope: "Independent replay covered the same narrowed request.".to_string(),
            root_cause_analysis: "Missing backend validation".to_string(),
            improvement_suggestions: "Add a check in the API gateway".to_string(),
            submission_grade_review: SubmissionGradeReview {
                verdict: SubmissionGradeVerdict::SubmissionGrade,
                summary: "All hard gates are present.".to_string(),
                hard_gates: vec![SubmissionGradeGate {
                    name: "Documentation and contract binding".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                soft_gates: vec![SubmissionGradeGate {
                    name: "Report readability".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                direct_fail_reasons: Vec::new(),
            },
        };
        assert!(report.validate().is_ok());
    }

    #[test]
    fn test_bug_report_validation_failure() {
        let mut report = BugReport {
            title: "Dimension mismatch issue".to_string(),
            target: "milvus".to_string(),
            version: "2.0.0".to_string(),
            defect_type: DefectType::PoorDiagnostics,
            doc_citation_url: "".to_string(), // Invalid
            contract_assertions: vec!["Dimension must be greater than 0".to_string()],
            surviving_assertions: vec!["Dimension must be greater than 0".to_string()],
            mre_code: "client.create_collection(dimension=-1)".to_string(),
            rerun_instructions: "Replace placeholder".to_string(),
            initial_run_summary: "Initial run classified as PoorDiagnostics".to_string(),
            reproduction_summary: "repro_1 and repro_2 matched".to_string(),
            classification_basis: "Three consistent classifications".to_string(),
            runtime_evidence: "stdout/stderr".to_string(),
            independent_review_summary: "Independent replay confirmed the same issue.".to_string(),
            review_scope: "Independent replay covered the same narrowed request.".to_string(),
            root_cause_analysis: "Missing backend validation".to_string(),
            improvement_suggestions: "Add a check in the API gateway".to_string(),
            submission_grade_review: SubmissionGradeReview {
                verdict: SubmissionGradeVerdict::SubmissionGrade,
                summary: "All hard gates are present.".to_string(),
                hard_gates: vec![SubmissionGradeGate {
                    name: "Documentation and contract binding".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                soft_gates: vec![SubmissionGradeGate {
                    name: "Report readability".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                direct_fail_reasons: Vec::new(),
            },
        };
        assert!(report.validate().is_err());
        
        report.doc_citation_url = "https://milvus.io/docs".to_string();
        report.mre_code = "".to_string(); // Invalid
        assert!(report.validate().is_err());
    }

    #[test]
    fn test_export_to_markdown() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("report.md");

        let report = BugReport {
            title: "Test Bug".to_string(),
            target: "qdrant".to_string(),
            version: "1.17.1".to_string(),
            defect_type: DefectType::IllegalSuccess,
            doc_citation_url: "http://example.com".to_string(),
            contract_assertions: vec!["Some rules".to_string()],
            surviving_assertions: vec!["Some rules".to_string()],
            mre_code: "print('bug')".to_string(),
            rerun_instructions: "Replace placeholder".to_string(),
            initial_run_summary: "Initial run matched".to_string(),
            reproduction_summary: "repro_1 and repro_2 matched".to_string(),
            classification_basis: "Three consistent classifications".to_string(),
            runtime_evidence: "stdout/stderr".to_string(),
            independent_review_summary: "Independent replay confirmed the same issue.".to_string(),
            review_scope: "Independent replay covered the same narrowed request.".to_string(),
            root_cause_analysis: "Bug is here".to_string(),
            improvement_suggestions: "Fix it".to_string(),
            submission_grade_review: SubmissionGradeReview {
                verdict: SubmissionGradeVerdict::SubmissionGrade,
                summary: "All hard gates are present.".to_string(),
                hard_gates: vec![SubmissionGradeGate {
                    name: "Documentation and contract binding".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                soft_gates: vec![SubmissionGradeGate {
                    name: "Report readability".to_string(),
                    passed: true,
                    detail: "ok".to_string(),
                }],
                direct_fail_reasons: Vec::new(),
            },
        };

        report.export_to_markdown(&file_path).unwrap();
        let content = fs::read_to_string(&file_path).unwrap();
        
        assert!(content.contains("# Test Bug"));
        assert!(content.contains("http://example.com"));
        assert!(content.contains("print('bug')"));
    }

    #[test]
    fn test_from_verified_candidate() {
        let candidate = CandidateDefect {
            target: "qdrant".to_string(),
            version: "1.17.1".to_string(),
            defect_type: DefectType::IllegalSuccess,
            doc_citation_url: "http://example.com".to_string(),
            contract_assertions: vec!["limit must be positive".to_string()],
            surviving_assertions: vec!["limit must be positive".to_string()],
            mre_code: "requests.get('http://db:6333')".to_string(),
            initial_run: RunEvidence {
                phase: "initial".to_string(),
                db_url: "http://db:6333".to_string(),
                stdout: "".to_string(),
                stderr: "".to_string(),
                classifier_reason: "Initial run matched".to_string(),
                classifier_evidence_excerpt: "marker".to_string(),
            },
            reproduction_runs: vec![
                RunEvidence {
                    phase: "repro_1".to_string(),
                    db_url: "http://db:6333".to_string(),
                    stdout: "".to_string(),
                    stderr: "".to_string(),
                    classifier_reason: "repro_1 matched".to_string(),
                    classifier_evidence_excerpt: "marker".to_string(),
                },
                RunEvidence {
                    phase: "repro_2".to_string(),
                    db_url: "http://db:6333".to_string(),
                    stdout: "".to_string(),
                    stderr: "".to_string(),
                    classifier_reason: "repro_2 matched".to_string(),
                    classifier_evidence_excerpt: "marker".to_string(),
                },
            ],
            status: CandidateStatus::ReproducedTwice,
            independent_review_summary: Some(
                "Independent replay confirmed the same offset diagnostics issue on fresh sandboxes."
                    .to_string(),
            ),
            review_scope: Some(
                "Fresh replay covered collection setup, seed insert, and the narrowed failing request."
                    .to_string(),
            ),
            downgrade_reason: None,
        };

        let report = BugReport::from_verified_candidate(&candidate).unwrap();
        assert_eq!(report.target, "qdrant");
        assert_eq!(
            report.submission_grade_review.verdict,
            SubmissionGradeVerdict::SubmissionGrade
        );
    }

    #[test]
    fn test_submission_grade_review_fails_without_independent_review() {
        let report = BugReport {
            title: "Test Bug".to_string(),
            target: "qdrant".to_string(),
            version: "1.17.1".to_string(),
            defect_type: DefectType::PoorDiagnostics,
            doc_citation_url: "https://qdrant.tech/documentation/concepts/search/".to_string(),
            contract_assertions: vec!["offset must be >= 0".to_string()],
            surviving_assertions: vec!["offset must be >= 0".to_string()],
            mre_code: "BASE_URL = \"{{TESTVDB_DB_URL}}\"".to_string(),
            rerun_instructions: "Replace placeholder".to_string(),
            initial_run_summary: "Initial run matched".to_string(),
            reproduction_summary: "repro_1 matched; repro_2 matched".to_string(),
            classification_basis: "Three consistent classifications".to_string(),
            runtime_evidence: "Initial Evidence Excerpt:\nfoo\nReproductions:\nbar".to_string(),
            independent_review_summary: BugReport::default_independent_review_summary(),
            review_scope: BugReport::default_review_scope(),
            root_cause_analysis: "The error omits offset.".to_string(),
            improvement_suggestions: "Mention offset in the error.".to_string(),
            submission_grade_review: SubmissionGradeReview {
                verdict: SubmissionGradeVerdict::NeedsRewrite,
                summary: String::new(),
                hard_gates: Vec::new(),
                soft_gates: Vec::new(),
                direct_fail_reasons: Vec::new(),
            },
        };
        let review = report.evaluate_submission_grade();
        assert_eq!(review.verdict, SubmissionGradeVerdict::NeedsRewrite);
        assert!(
            review
                .direct_fail_reasons
                .iter()
                .any(|reason| reason.contains("Double reproduction and independent review"))
        );
        assert!(report.reproduction_summary.contains("repro_1 matched"));
    }
}
