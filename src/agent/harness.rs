use crate::agent::llm::DeepSeekClient;
use crate::report::llm_analysis::{analyze_defect_with_client, RootCauseAnalysis};

#[derive(Debug, Clone)]
pub struct ReviewRecord {
    pub test_case_summary: String,
    pub defect_type: String,
    pub analysis: RootCauseAnalysis,
    pub outcome: ReviewOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewOutcome {
    ConfirmedDefect,
    FalsePositive,
    Uncertain,
}

pub type VerifiedDefect = String;

pub struct Harness {
    pub review_history: Vec<ReviewRecord>,
}

impl Harness {
    pub fn new() -> Self {
        Harness {
            review_history: Vec::new(),
        }
    }

    pub async fn review(
        &mut self,
        client: &DeepSeekClient,
        stdout: &str,
        stderr: &str,
        defect_type: &str,
        mre_code: &str,
        test_case_summary: &str,
    ) -> ReviewRecord {
        let analysis = analyze_defect_with_client(
            client,
            stdout,
            stderr,
            defect_type,
            mre_code,
        )
        .await;

        let outcome = if analysis.is_real_defect && analysis.confidence >= 0.7 {
            ReviewOutcome::ConfirmedDefect
        } else if !analysis.is_real_defect && analysis.confidence >= 0.7 {
            ReviewOutcome::FalsePositive
        } else {
            ReviewOutcome::Uncertain
        };

        let record = ReviewRecord {
            test_case_summary: test_case_summary.to_string(),
            defect_type: defect_type.to_string(),
            analysis,
            outcome,
        };

        self.review_history.push(record.clone());
        record
    }

    pub async fn review_or_fallback(
        &mut self,
        client: &DeepSeekClient,
        stdout: &str,
        stderr: &str,
        defect_type: &str,
        mre_code: &str,
        test_case_summary: &str,
    ) -> ReviewRecord {
        let analysis = analyze_defect_with_client(
            client,
            stdout,
            stderr,
            defect_type,
            mre_code,
        ).await;

        // 检查 LLM 是否返回了有效结果
        // 如果 root_cause 是 "LLM analysis unavailable"（回退标记）或 confidence == 0.0
        // 则判定为 Uncertain（降级到 classifier）
        let is_fallback = analysis.confidence == 0.0
            || analysis.root_cause == "LLM analysis unavailable";

        let outcome = if is_fallback {
            ReviewOutcome::Uncertain
        } else if analysis.is_real_defect && analysis.confidence >= 0.7 {
            ReviewOutcome::ConfirmedDefect
        } else if !analysis.is_real_defect && analysis.confidence >= 0.7 {
            ReviewOutcome::FalsePositive
        } else {
            ReviewOutcome::Uncertain
        };

        let record = ReviewRecord {
            test_case_summary: test_case_summary.to_string(),
            defect_type: defect_type.to_string(),
            analysis,
            outcome,
        };

        self.review_history.push(record.clone());
        record
    }

    pub fn build_review_context(&self) -> String {
        if self.review_history.is_empty() {
            return String::new();
        }

        let recent: Vec<&ReviewRecord> = self
            .review_history
            .iter()
            .rev()
            .take(3)
            .collect();

        let mut ctx = String::from("\n\n=== PREVIOUS ROUND DEFECT REVIEW ===\n");
        for record in recent.iter().rev() {
            let outcome_label = match record.outcome {
                ReviewOutcome::ConfirmedDefect => "CONFIRMED DEFECT",
                ReviewOutcome::FalsePositive => "FALSE POSITIVE",
                ReviewOutcome::Uncertain => "UNCERTAIN",
            };
            let ctx_line = format!(
                "- {}: {} -> {} (confidence: {:.2})\n  {}\n",
                record.test_case_summary,
                record.defect_type,
                outcome_label,
                record.analysis.confidence,
                record.analysis.root_cause,
            );
            if ctx.len() + ctx_line.len() > 500 {
                break;
            }
            ctx.push_str(&ctx_line);
        }
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::llm_analysis::RootCauseAnalysis;

    fn make_analysis(is_real_defect: bool, confidence: f32) -> RootCauseAnalysis {
        RootCauseAnalysis {
            is_real_defect,
            defect_type: Some("STATE_LOGIC_VIOLATION".to_string()),
            root_cause: "test root cause".to_string(),
            confidence,
            fixed_script: None,
        }
    }

    #[test]
    fn test_review_outcome_confirmed() {
        let analysis = make_analysis(true, 0.9);
        let record = ReviewRecord {
            test_case_summary: "test1".to_string(),
            defect_type: "STATE_LOGIC_VIOLATION".to_string(),
            analysis,
            outcome: ReviewOutcome::ConfirmedDefect,
        };
        assert_eq!(record.outcome, ReviewOutcome::ConfirmedDefect);
    }

    #[test]
    fn test_review_outcome_false_positive() {
        let analysis = make_analysis(false, 0.9);
        let record = ReviewRecord {
            test_case_summary: "test2".to_string(),
            defect_type: "STATE_LOGIC_VIOLATION".to_string(),
            analysis,
            outcome: ReviewOutcome::FalsePositive,
        };
        assert_eq!(record.outcome, ReviewOutcome::FalsePositive);
    }

    #[test]
    fn test_review_outcome_uncertain() {
        let analysis = make_analysis(true, 0.5);
        let record = ReviewRecord {
            test_case_summary: "test3".to_string(),
            defect_type: "STATE_LOGIC_VIOLATION".to_string(),
            analysis,
            outcome: ReviewOutcome::Uncertain,
        };
        assert_eq!(record.outcome, ReviewOutcome::Uncertain);
    }

    #[test]
    fn test_review_context_limit() {
        let mut harness = Harness::new();
        // Push 5 records; only the 3 most recent should appear
        for i in 0..5 {
            harness.review_history.push(ReviewRecord {
                test_case_summary: format!("test{}", i),
                defect_type: "STATE_LOGIC_VIOLATION".to_string(),
                analysis: make_analysis(true, 0.8),
                outcome: ReviewOutcome::ConfirmedDefect,
            });
        }
        let ctx = harness.build_review_context();
        // Should contain test4, test3, test2 (the 3 most recent), not test0, test1
        assert!(ctx.contains("test4"));
        assert!(ctx.contains("test3"));
        assert!(ctx.contains("test2"));
        assert!(!ctx.contains("test0"));
        assert!(!ctx.contains("test1"));
        // Should not exceed 500 characters
        assert!(ctx.len() <= 500);
    }

    #[test]
    fn test_empty_review_context() {
        let harness = Harness::new();
        let ctx = harness.build_review_context();
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_context_truncates_at_500_chars() {
        let mut harness = Harness::new();
        // Push a record with a very long root cause
        harness.review_history.push(ReviewRecord {
            test_case_summary: "long_test".to_string(),
            defect_type: "STATE_LOGIC_VIOLATION".to_string(),
            analysis: RootCauseAnalysis {
                is_real_defect: true,
                defect_type: Some("STATE_LOGIC_VIOLATION".to_string()),
                root_cause: "A".repeat(600),
                confidence: 0.9,
                fixed_script: None,
            },
            outcome: ReviewOutcome::ConfirmedDefect,
        });
        let ctx = harness.build_review_context();
        // Should still be within 500 chars
        assert!(ctx.len() <= 500);
    }

    #[test]
    fn test_review_or_fallback_api_failure() {
        // Mock scenario: confidence==0.0 should result in ReviewOutcome::Uncertain
        let analysis = RootCauseAnalysis {
            is_real_defect: true,
            defect_type: Some("STATE_LOGIC_VIOLATION".to_string()),
            root_cause: "API call failed".to_string(),
            confidence: 0.0,
            fixed_script: None,
        };
        let record = ReviewRecord {
            test_case_summary: "api_failure".to_string(),
            defect_type: "STATE_LOGIC_VIOLATION".to_string(),
            analysis,
            outcome: ReviewOutcome::Uncertain,
        };
        assert_eq!(record.outcome, ReviewOutcome::Uncertain);
    }

    #[test]
    fn test_review_or_fallback_empty_output() {
        // root_cause=="LLM analysis unavailable" should result in ReviewOutcome::Uncertain
        let analysis = RootCauseAnalysis {
            is_real_defect: false,
            defect_type: None,
            root_cause: "LLM analysis unavailable".to_string(),
            confidence: 0.0,
            fixed_script: None,
        };
        let record = ReviewRecord {
            test_case_summary: "empty_output".to_string(),
            defect_type: "unknown".to_string(),
            analysis,
            outcome: ReviewOutcome::Uncertain,
        };
        assert_eq!(record.outcome, ReviewOutcome::Uncertain);
    }
}