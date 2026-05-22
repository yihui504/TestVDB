use crate::agent::classifier::DefectType;
use crate::agent::oracle::OracleFinding;
use crate::contract::schema::BehaviorTestResult;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Copy)]
pub enum TestResult {
    Pass,
    Rejected,
    Defect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamResult {
    pub param_name: String,
    pub endpoint: String,
    pub result: TestResult,
    pub defect_type: Option<DefectType>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EndpointCov {
    pub endpoint: String,
    pub params_tested: usize,
    pub params_total: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StrategyStats {
    pub total_tests: usize,
    pub defects_found: usize,
    pub rejections: usize,
    pub script_errors: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExplorationState {
    pub tested_params: Vec<ParamResult>,
    pub endpoint_coverage: Vec<EndpointCov>,
    pub strategy_effectiveness: StrategyStats,
    pub consecutive_no_defect: usize,
    #[serde(default)]
    pub oracle_findings: Vec<OracleFinding>,
    #[serde(default)]
    pub tested_behaviors: Vec<BehaviorTestResult>,
}

impl ExplorationState {
    pub fn new() -> Self {
        ExplorationState::default()
    }

    pub fn record_test(
        &mut self,
        param_name: &str,
        endpoint: &str,
        result: TestResult,
        defect_type: Option<DefectType>,
    ) {
        self.tested_params.push(ParamResult {
            param_name: param_name.to_string(),
            endpoint: endpoint.to_string(),
            result,
            defect_type,
        });
        self.strategy_effectiveness.total_tests += 1;
        match &result {
            TestResult::Rejected => {
                self.strategy_effectiveness.rejections += 1;
                self.consecutive_no_defect += 1;
            }
            TestResult::Defect => {
                self.strategy_effectiveness.defects_found += 1;
                self.consecutive_no_defect = 0;
            }
            TestResult::Pass => {
                self.consecutive_no_defect += 1;
            }
        }
    }

    pub fn record_script_error(&mut self) {
        self.strategy_effectiveness.total_tests += 1;
        self.strategy_effectiveness.script_errors += 1;
    }

    pub fn record_oracle_findings(&mut self, findings: Vec<OracleFinding>) {
        for f in &findings {
            if f.violated {
                self.strategy_effectiveness.defects_found += 1;
                self.consecutive_no_defect = 0;
            }
        }
        self.oracle_findings.extend(findings);
    }

    pub fn oracle_violations(&self) -> Vec<&OracleFinding> {
        self.oracle_findings.iter().filter(|f| f.violated).collect()
    }

    pub fn oracle_violations_owned(&self) -> Vec<OracleFinding> {
        self.oracle_findings.iter().filter(|f| f.violated).cloned().collect()
    }

    pub fn tested_param_names(&self) -> HashSet<String> {
        self.tested_params
            .iter()
            .map(|p| p.param_name.to_lowercase())
            .collect()
    }

    pub fn unique_assertions_count(&self) -> usize {
        self.tested_param_names().len()
    }

    pub fn record_assertion(&mut self, key: &str) {
        if !self.tested_param_names().contains(key) {
            self.tested_params.push(ParamResult {
                param_name: key.to_string(),
                endpoint: "sequence_tool".to_string(),
                result: TestResult::Pass,
                defect_type: None,
            });
        }
    }

    pub fn to_prompt_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_defect_resets_consecutive() {
        let mut state = ExplorationState::new();
        state.record_test("limit", "search", TestResult::Rejected, None);
        assert_eq!(state.consecutive_no_defect, 1);
        state.record_test("offset", "search", TestResult::Defect, Some(DefectType::IllegalSuccess));
        assert_eq!(state.consecutive_no_defect, 0);
        assert_eq!(state.strategy_effectiveness.defects_found, 1);
    }

    #[test]
    fn test_record_rejection_increments_consecutive() {
        let mut state = ExplorationState::new();
        state.record_test("limit", "search", TestResult::Rejected, None);
        state.record_test("offset", "search", TestResult::Rejected, None);
        assert_eq!(state.consecutive_no_defect, 2);
        assert_eq!(state.strategy_effectiveness.rejections, 2);
    }

    #[test]
    fn test_record_pass_increments_consecutive() {
        let mut state = ExplorationState::new();
        state.record_test("limit", "search", TestResult::Pass, None);
        assert_eq!(state.consecutive_no_defect, 1);
        assert_eq!(state.strategy_effectiveness.total_tests, 1);
    }

    #[test]
    fn test_script_error_tracking() {
        let mut state = ExplorationState::new();
        state.record_script_error();
        state.record_script_error();
        assert_eq!(state.strategy_effectiveness.script_errors, 2);
        assert_eq!(state.strategy_effectiveness.total_tests, 2);
    }

    #[test]
    fn test_unique_assertions_dedup() {
        let mut state = ExplorationState::new();
        state.record_test("limit", "search", TestResult::Rejected, None);
        state.record_test("limit", "search", TestResult::Rejected, None);
        assert_eq!(state.unique_assertions_count(), 1);
    }

    #[test]
    fn test_to_prompt_json_valid() {
        let mut state = ExplorationState::new();
        state.record_test("hnsw_ef", "search", TestResult::Defect, Some(DefectType::IllegalSuccess));
        let json = state.to_prompt_json();
        assert!(json.contains("hnsw_ef"));
        assert!(json.contains("Defect"));
    }
}
