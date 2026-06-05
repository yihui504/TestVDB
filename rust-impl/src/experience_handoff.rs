use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DefectPattern {
    pub endpoint: String,
    pub defect_type: String,
    pub trigger_pattern: String,
    pub verified: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoundExperience {
    pub round_number: usize,
    pub timestamp: String,
    pub explored_defect_patterns: Vec<DefectPattern>,
    pub covered_endpoints: Vec<String>,
    pub covered_params: Vec<String>,
    pub llm_conversation_summary: String,
    pub defects_found_this_round: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExperienceHandoff {
    pub target: String,
    pub version: String,
    pub rounds: Vec<RoundExperience>,
    pub total_defects_found: usize,
    pub total_rounds_completed: usize,
}

impl ExperienceHandoff {
    pub fn new(target: &str, version: &str) -> Self {
        Self {
            target: target.to_string(),
            version: version.to_string(),
            rounds: Vec::new(),
            total_defects_found: 0,
            total_rounds_completed: 0,
        }
    }

    pub fn load(target: &str, version: &str) -> Option<Self> {
        let path = handoff_path();
        if !Path::new(&path).exists() {
            return None;
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(exp) => {
                    if exp.target == target && exp.version == version {
                        info!(
                            "Loaded experience handoff: {} rounds, {} defects",
                            exp.total_rounds_completed, exp.total_defects_found
                        );
                        Some(exp)
                    } else {
                        warn!(
                            "Experience handoff target/version mismatch (expected {}/{} got {}/{}). Starting fresh.",
                            target, version, exp.target, exp.version
                        );
                        None
                    }
                }
                Err(e) => {
                    warn!("Failed to parse experience handoff: {}. Starting fresh.", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to read experience handoff: {}. Starting fresh.", e);
                None
            }
        }
    }

    pub fn add_round(&mut self, round: RoundExperience) {
        self.total_defects_found += round.defects_found_this_round;
        self.rounds.push(round);
        self.total_rounds_completed = self.rounds.len();
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = handoff_path();
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize experience handoff")?;
        std::fs::write(&path, &content)
            .context("Failed to write experience handoff file")?;
        info!("Experience handoff saved to {} ({} rounds, {} defects)",
            path, self.total_rounds_completed, self.total_defects_found);
        Ok(())
    }

    pub fn already_explored(&self, endpoint: &str, defect_type: &str, trigger_pattern: &str) -> bool {
        self.rounds
            .iter()
            .flat_map(|r| &r.explored_defect_patterns)
            .any(|p| p.endpoint == endpoint && p.defect_type == defect_type && p.trigger_pattern == trigger_pattern)
    }

    pub fn build_llm_context(&self) -> String {
        if self.rounds.is_empty() {
            return String::new();
        }

        let mut ctx = String::from("\n\n## PREVIOUS EXPLORATION EXPERIENCE (DO NOT RETEST)\n\n");
        ctx.push_str(&format!(
            "You have already completed {} rounds of exploration on {} version {}.\n",
            self.total_rounds_completed, self.target, self.version
        ));
        ctx.push_str(&format!(
            "Total defects found across all rounds: {}.\n\n",
            self.total_defects_found
        ));

        ctx.push_str("### Previously Explored Defect Patterns (avoid repeating these exact patterns):\n");
        let all_patterns: Vec<&DefectPattern> = self
            .rounds
            .iter()
            .flat_map(|r| &r.explored_defect_patterns)
            .collect();

        let mut seen = std::collections::HashSet::new();
        for p in &all_patterns {
            let key = format!("{}|{}|{}", p.endpoint, p.defect_type, p.trigger_pattern);
            if seen.insert(key) {
                let verified_mark = if p.verified { "[VERIFIED]" } else { "[UNVERIFIED]" };
                ctx.push_str(&format!(
                    "- {} Endpoint: {} | Type: {} | Trigger: {}\n",
                    verified_mark, p.endpoint, p.defect_type, p.trigger_pattern
                ));
            }
        }

        ctx.push_str(&format!("\n### Previously Covered Endpoints ({} total):\n", self.rounds.iter().flat_map(|r| &r.covered_endpoints).collect::<std::collections::HashSet<_>>().len()));
        let mut all_endpoints: Vec<&String> = self
            .rounds
            .iter()
            .flat_map(|r| &r.covered_endpoints)
            .collect();
        all_endpoints.sort();
        all_endpoints.dedup();
        for ep in &all_endpoints {
            ctx.push_str(&format!("- {}\n", ep));
        }

        if let Some(last_round) = self.rounds.last() {
            if !last_round.llm_conversation_summary.is_empty() {
                ctx.push_str("\n### Key Findings from Last Round:\n");
                ctx.push_str(&last_round.llm_conversation_summary);
                ctx.push('\n');
            }
        }

        ctx.push_str("\n**IMPORTANT**: Focus on NEW, unexplored patterns. Do NOT retest patterns listed above.\n");

        ctx
    }
}

#[cfg(not(test))]
fn handoff_path() -> String {
    "experience_handoff.json".to_string()
}

#[cfg(test)]
fn handoff_path() -> String {
    format!("experience_handoff_test_{:?}.json", std::thread::current().id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_handoff() {
        let exp = ExperienceHandoff::new("milvus", "v2.4.0");
        assert_eq!(exp.target, "milvus");
        assert_eq!(exp.version, "v2.4.0");
        assert_eq!(exp.total_rounds_completed, 0);
        assert_eq!(exp.total_defects_found, 0);
    }

    #[test]
    fn test_add_round() {
        let mut exp = ExperienceHandoff::new("milvus", "v2.4.0");
        let round = RoundExperience {
            round_number: 1,
            timestamp: "2026-06-01".to_string(),
            explored_defect_patterns: vec![DefectPattern {
                endpoint: "/collections".to_string(),
                defect_type: "IllegalSuccess".to_string(),
                trigger_pattern: "dim=-1".to_string(),
                verified: true,
            }],
            covered_endpoints: vec!["/collections".to_string()],
            covered_params: vec!["dim".to_string()],
            llm_conversation_summary: "Found dimension boundary issue".to_string(),
            defects_found_this_round: 1,
        };
        exp.add_round(round);
        assert_eq!(exp.total_rounds_completed, 1);
        assert_eq!(exp.total_defects_found, 1);
    }

    #[test]
    fn test_already_explored() {
        let mut exp = ExperienceHandoff::new("milvus", "v2.4.0");
        let round = RoundExperience {
            round_number: 1,
            timestamp: "2026-06-01".to_string(),
            explored_defect_patterns: vec![DefectPattern {
                endpoint: "/collections".to_string(),
                defect_type: "IllegalSuccess".to_string(),
                trigger_pattern: "dim=-1".to_string(),
                verified: true,
            }],
            covered_endpoints: vec!["/collections".to_string()],
            covered_params: vec!["dim".to_string()],
            llm_conversation_summary: String::new(),
            defects_found_this_round: 1,
        };
        exp.add_round(round);

        assert!(exp.already_explored("/collections", "IllegalSuccess", "dim=-1"));
        assert!(!exp.already_explored("/collections", "IllegalSuccess", "different_trigger"));
        assert!(!exp.already_explored("/search", "IllegalSuccess", "dim=-1"));
    }

    #[test]
    fn test_build_llm_context() {
        let mut exp = ExperienceHandoff::new("milvus", "v2.4.0");
        let round = RoundExperience {
            round_number: 1,
            timestamp: "2026-06-01".to_string(),
            explored_defect_patterns: vec![DefectPattern {
                endpoint: "/collections".to_string(),
                defect_type: "IllegalSuccess".to_string(),
                trigger_pattern: "dim=-1".to_string(),
                verified: true,
            }],
            covered_endpoints: vec!["/collections".to_string()],
            covered_params: vec!["dim".to_string()],
            llm_conversation_summary: "Found dimension boundary issue".to_string(),
            defects_found_this_round: 1,
        };
        exp.add_round(round);

        let ctx = exp.build_llm_context();
        assert!(ctx.contains("PREVIOUS EXPLORATION EXPERIENCE"));
        assert!(ctx.contains("dim=-1"));
        assert!(ctx.contains("VERIFIED"));
        assert!(ctx.contains("DO NOT RETEST"));
    }

    #[test]
    fn test_save_and_load() {
        let mut exp = ExperienceHandoff::new("test_db", "v1.0");
        let round = RoundExperience {
            round_number: 1,
            timestamp: "2026-06-01".to_string(),
            explored_defect_patterns: vec![DefectPattern {
                endpoint: "/test".to_string(),
                defect_type: "IllegalSuccess".to_string(),
                trigger_pattern: "test_pattern".to_string(),
                verified: true,
            }],
            covered_endpoints: vec!["/test".to_string()],
            covered_params: vec!["param".to_string()],
            llm_conversation_summary: "Test".to_string(),
            defects_found_this_round: 1,
        };
        exp.add_round(round);
        exp.save().unwrap();

        let loaded = ExperienceHandoff::load("test_db", "v1.0").unwrap();
        assert_eq!(loaded.total_rounds_completed, 1);
        assert_eq!(loaded.total_defects_found, 1);
        assert!(loaded.already_explored("/test", "IllegalSuccess", "test_pattern"));

        std::fs::remove_file(handoff_path()).ok();
    }

    #[test]
    fn test_load_nonexistent() {
        let path = handoff_path();
        std::fs::remove_file(&path).ok();
        assert!(ExperienceHandoff::load("nonexistent", "v0").is_none());
    }

    #[test]
    fn test_load_mismatched_target() {
        let mut exp = ExperienceHandoff::new("db_a", "v1.0");
        exp.add_round(RoundExperience {
            round_number: 1,
            timestamp: "2026-06-01".to_string(),
            explored_defect_patterns: vec![],
            covered_endpoints: vec![],
            covered_params: vec![],
            llm_conversation_summary: String::new(),
            defects_found_this_round: 0,
        });
        exp.save().unwrap();

        assert!(ExperienceHandoff::load("db_b", "v1.0").is_none());

        std::fs::remove_file(handoff_path()).ok();
    }
}