use crate::agent::orchestrator::CollectedDefect;
use crate::contract::analyzer::BatchDefect;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

const STATE_FILE: &str = ".mine_state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MinePhase {
    ContractLoaded,
    Generators,
    Orchestrator,
    Verification,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineState {
    pub target: String,
    pub version: String,
    pub phase: MinePhase,
    pub max_rounds: usize,
    pub llm_turns: usize,
    pub strategy_threshold: usize,
    pub skip_generators: bool,
    pub skip_verify: bool,
    pub multi_defect: bool,
    pub shadow: bool,
    pub skip_safety_nets: bool,
    #[serde(default)]
    pub contract_content: String,
    #[serde(default)]
    pub current_round: usize,
    #[serde(default)]
    pub all_round_defects: Vec<Vec<BatchDefect>>,
    #[serde(default)]
    pub low_priority_defects: Vec<BatchDefect>,
    #[serde(default)]
    pub converged_at: Option<usize>,
    #[serde(default)]
    pub orchestrator_defects: Vec<CollectedDefect>,
    #[serde(default)]
    pub mine_defect_count: usize,
}

impl MineState {
    pub fn new(
        target: &str,
        version: &str,
        contract_content: &str,
        max_rounds: usize,
        llm_turns: usize,
        strategy_threshold: usize,
        skip_generators: bool,
        skip_verify: bool,
        multi_defect: bool,
        shadow: bool,
        skip_safety_nets: bool,
    ) -> Self {
        MineState {
            target: target.to_string(),
            version: version.to_string(),
            phase: MinePhase::ContractLoaded,
            contract_content: contract_content.to_string(),
            max_rounds,
            llm_turns,
            strategy_threshold,
            skip_generators,
            skip_verify,
            multi_defect,
            shadow,
            skip_safety_nets,
            current_round: 0,
            all_round_defects: Vec::new(),
            low_priority_defects: Vec::new(),
            converged_at: None,
            orchestrator_defects: Vec::new(),
            mine_defect_count: 0,
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(STATE_FILE, json)?;
        info!(
            "Mine state saved: target={}, version={}, phase={:?}",
            self.target, self.version, self.phase
        );
        Ok(())
    }

    pub fn try_load(target: &str, version: &str) -> Option<Self> {
        let path = Path::new(STATE_FILE);
        if !path.exists() {
            return None;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<MineState>(&content) {
                Ok(state) => {
                    if state.target == target && state.version == version {
                        info!(
                            "Mine state loaded: target={}, version={}, phase={:?}, round={}",
                            state.target, state.version, state.phase, state.current_round
                        );
                        Some(state)
                    } else {
                        warn!(
                            "Mine state file exists but target/version mismatch: expected {}/{} found {}/{}",
                            target, version, state.target, state.version
                        );
                        None
                    }
                }
                Err(e) => {
                    warn!("Failed to parse mine state file: {}", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to read mine state file: {}", e);
                None
            }
        }
    }

    pub fn cleanup() {
        let path = Path::new(STATE_FILE);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(path) {
                warn!("Failed to cleanup mine state file: {}", e);
            } else {
                info!("Mine state file cleaned up");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mine_state_roundtrip() {
        let state = MineState::new(
            "milvus", "v2.6.8", "{}", 3, 10, 5,
            false, false, false, false, false,
        );
        let json = serde_json::to_string(&state).unwrap();
        let restored: MineState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.target, "milvus");
        assert_eq!(restored.version, "v2.6.8");
        assert_eq!(restored.phase, MinePhase::ContractLoaded);
        assert_eq!(restored.current_round, 0);
        assert!(restored.all_round_defects.is_empty());
    }

    #[test]
    fn test_mine_state_with_defects() {
        let mut state = MineState::new(
            "milvus", "v2.6.8", "{}", 3, 10, 5,
            false, false, false, false, false,
        );
        state.phase = MinePhase::Generators;
        state.current_round = 2;
        state.all_round_defects = vec![vec![], vec![]];
        let json = serde_json::to_string(&state).unwrap();
        let restored: MineState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.current_round, 2);
        assert_eq!(restored.all_round_defects.len(), 2);
    }

    #[test]
    fn test_mine_state_phase_serialization() {
        let state = MineState::new(
            "milvus", "v2.6.8", "{}", 3, 10, 5,
            false, false, false, false, false,
        );
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("ContractLoaded"));

        let mut state = MineState::new(
            "milvus", "v2.6.8", "{}", 3, 10, 5,
            false, false, false, false, false,
        );
        state.phase = MinePhase::Complete;
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("Complete"));
    }
}