use std::collections::HashSet;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CoverageTracker {
    visited: HashSet<String>,
    pub endpoints: Vec<ApiEndpoint>,
}

impl CoverageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_endpoint(&mut self, endpoint: ApiEndpoint) {
        self.endpoints.push(endpoint);
    }

    pub fn record_visit(&mut self, endpoint: &str, param: &str, value: &str) {
        let key = format!("{}|{}|{}", endpoint, param, value);
        self.visited.insert(key);
    }

    pub fn visited_count(&self) -> usize {
        self.visited.len()
    }

    pub fn endpoint_count(&self) -> usize {
        self.endpoints.iter().map(|e| e.params.len()).sum()
    }

    pub fn total_params(&self) -> usize {
        self.endpoints.iter().map(|e| e.params.len()).sum()
    }

    pub fn unvisited_params(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut covered_pairs = HashSet::new();
        for key in &self.visited {
            let parts: Vec<&str> = key.splitn(3, '|').collect();
            if parts.len() >= 2 {
                covered_pairs.insert(format!("{}|{}", parts[0], parts[1]));
            }
        }
        for ep in &self.endpoints {
            for param in &ep.params {
                let pair = format!("{}|{}", ep.path, param);
                if !covered_pairs.contains(&pair) {
                    result.push((ep.path.clone(), param.clone()));
                }
            }
        }
        result
    }

    pub fn unvisited_params_for_endpoint(&self, target_endpoint: &str) -> Vec<String> {
        let mut covered_params = HashSet::new();
        for key in &self.visited {
            let parts: Vec<&str> = key.splitn(3, '|').collect();
            if parts.len() >= 2 && parts[0] == target_endpoint {
                covered_params.insert(parts[1].to_string());
            }
        }
        self.endpoints
            .iter()
            .filter(|ep| ep.path == target_endpoint)
            .flat_map(|ep| ep.params.iter().filter(|p| !covered_params.contains(*p)).cloned())
            .collect()
    }

    pub fn coverage_ratio(&self) -> f64 {
        if self.endpoints.is_empty() {
            return 0.0;
        }
        let total = self.total_params();
        if total == 0 {
            return 0.0;
        }
        let mut covered = HashSet::new();
        for key in &self.visited {
            let parts: Vec<&str> = key.splitn(3, '|').collect();
            if parts.len() >= 2 {
                covered.insert(format!("{}|{}", parts[0], parts[1]));
            }
        }
        covered.len() as f64 / total as f64
    }

    pub fn report(&self) -> String {
        let ratio = self.coverage_ratio();
        let unvisited = self.unvisited_params();
        let mut report = format!("API Coverage: {:.1}% ({} visited entries)\n", ratio * 100.0, self.visited.len());
        if !unvisited.is_empty() {
            report.push_str("Unvisited params:\n");
            for (ep, param) in unvisited.iter().take(20) {
                report.push_str(&format!("  {} / {}\n", ep, param));
            }
            if unvisited.len() > 20 {
                report.push_str(&format!("  ... and {} more\n", unvisited.len() - 20));
            }
        }
        report
    }
}

pub const PATTERN_CATEGORIES: [&str; 21] = [
    "count_consistency",
    "data_visibility",
    "state_residual",
    "idempotency",
    "search_correctness",
    "partition_isolation",
    "alias_state",
    "index_state",
    "concurrent_insert_count",
    "concurrent_upsert_duplicate",
    "concurrent_delete_stale",
    "concurrent_create_conflict",
    "concurrent_mixed_ops",
    "flush_visibility",
    "load_search_failure",
    "delete_stale_read",
    "index_immediate_use",
    "compact_immediate_effect",
    "cross_endpoint_chain",
    "semantic_equivalence",
    "boundary_deepening",
];

#[derive(Debug, Clone, Default)]
pub struct PatternTracker {
    explored: HashSet<String>,
}

impl PatternTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_pattern(&mut self, pattern: &str) {
        if PATTERN_CATEGORIES.contains(&pattern) {
            self.explored.insert(pattern.to_string());
        }
    }

    pub fn explored_patterns(&self) -> Vec<&str> {
        PATTERN_CATEGORIES
            .iter()
            .filter(|p| self.explored.contains(**p))
            .copied()
            .collect()
    }

    pub fn unexplored_patterns(&self) -> Vec<&str> {
        PATTERN_CATEGORIES
            .iter()
            .filter(|p| !self.explored.contains(**p))
            .copied()
            .collect()
    }

    pub fn pattern_diversity_report(&self) -> String {
        let explored = self.explored_patterns();
        let unexplored = self.unexplored_patterns();
        format!(
            "Pattern Diversity: {}/21 patterns explored\nExplored: {}\nUNEXPLORED: {}",
            explored.len(),
            explored.join(", "),
            unexplored.join(", "),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_tracker() {
        let mut tracker = CoverageTracker::new();
        tracker.register_endpoint(ApiEndpoint {
            method: "POST".into(),
            path: "/collections/{name}/points/search".into(),
            params: vec!["limit".into(), "offset".into(), "score_threshold".into()],
        });
        tracker.record_visit("/collections/{name}/points/search", "limit", "0");
        tracker.record_visit("/collections/{name}/points/search", "limit", "-1");

        assert_eq!(tracker.visited_count(), 2);
        assert!(tracker.coverage_ratio() > 0.0);
        let unvisited = tracker.unvisited_params();
        assert!(unvisited.iter().any(|(_e, p)| p == "offset"));
        assert!(unvisited.iter().any(|(_e, p)| p == "score_threshold"));
    }

    #[test]
    fn test_pattern_tracker() {
        let mut tracker = PatternTracker::new();
        assert!(tracker.explored_patterns().is_empty());
        assert_eq!(tracker.unexplored_patterns().len(), 21);

        tracker.record_pattern("count_consistency");
        tracker.record_pattern("data_visibility");
        tracker.record_pattern("concurrent_insert_count");
        tracker.record_pattern("unknown_pattern");

        assert_eq!(tracker.explored_patterns().len(), 3);
        assert_eq!(tracker.unexplored_patterns().len(), 18);
        assert!(tracker.explored_patterns().contains(&"count_consistency"));
        assert!(!tracker.explored_patterns().contains(&"unknown_pattern"));

        let report = tracker.pattern_diversity_report();
        assert!(report.contains("Pattern Diversity: 3/21 patterns explored"));
        assert!(report.contains("Explored: count_consistency, data_visibility, concurrent_insert_count"));
        assert!(report.contains("UNEXPLORED:"));
    }

    #[test]
    fn test_pattern_tracker_duplicate() {
        let mut tracker = PatternTracker::new();
        tracker.record_pattern("idempotency");
        tracker.record_pattern("idempotency");
        assert_eq!(tracker.explored_patterns().len(), 1);
    }
}
