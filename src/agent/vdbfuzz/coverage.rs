use std::collections::HashSet;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageEntry {
    pub endpoint: String,
    pub param: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct CoverageTracker {
    visited: HashSet<String>,
    endpoints: Vec<ApiEndpoint>,
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
}
