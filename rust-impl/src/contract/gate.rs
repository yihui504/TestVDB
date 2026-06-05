use crate::agent::vdbfuzz::coverage::ApiEndpoint;
use crate::contract::schema::StructuredContract;
use crate::target::TargetPlugin;
use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::{info, warn};

const GATE_THRESHOLD: f64 = 90.0;

pub struct ContractGateResult {
    pub passed: bool,
    pub target: String,
    pub total_core_endpoints: usize,
    pub covered_endpoints: Vec<String>,
    pub missing_endpoints: Vec<String>,
    pub coverage_pct: f64,
    pub contract_groups: Vec<String>,
    pub admin_endpoints: Vec<String>,
}

pub struct ContractGate;

impl ContractGate {
    pub fn check(contract: &StructuredContract, plugin: &dyn TargetPlugin, target: &str) -> ContractGateResult {
        let all_endpoints = plugin.all_api_endpoints();

        let mut core_endpoints: Vec<&ApiEndpoint> = Vec::new();
        let mut admin_endpoints: Vec<String> = Vec::new();

        for ep in &all_endpoints {
            if Self::is_core_crud(ep) {
                core_endpoints.push(ep);
            } else {
                admin_endpoints.push(ep.path.clone());
            }
        }

        let contract_groups: Vec<String> = contract
            .api_endpoint
            .split('+')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut covered: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();

        for ep in &core_endpoints {
            if Self::is_endpoint_covered_by_groups(ep, &contract_groups) {
                covered.push(ep.path.clone());
            } else {
                missing.push(ep.path.clone());
            }
        }

        let total = core_endpoints.len();
        let covered_count = covered.len();
        let coverage_pct = if total > 0 {
            (covered_count as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        ContractGateResult {
            passed: coverage_pct >= GATE_THRESHOLD,
            target: target.to_string(),
            total_core_endpoints: total,
            covered_endpoints: covered,
            missing_endpoints: missing,
            coverage_pct,
            contract_groups,
            admin_endpoints,
        }
    }

    fn is_core_crud(ep: &ApiEndpoint) -> bool {
        let path = &ep.path;
        let admin_segments = [
            "/indexes/",
            "/partitions/",
            "/aliases/",
            "/load",
            "/release",
            "/flush",
            "/compact",
            "/meta",
            "/nodes",
            "/health",
            "/ready",
            "/livez",
            "/metrics",
            "/telemetry",
            "/cluster",
        ];
        for seg in &admin_segments {
            if path.contains(seg) {
                return false;
            }
        }
        true
    }

    fn is_endpoint_covered_by_groups(ep: &ApiEndpoint, groups: &[String]) -> bool {
        for group in groups {
            let keywords: Vec<&str> = group.split('_').filter(|k| !k.is_empty()).collect();
            if keywords.is_empty() {
                continue;
            }
            if keywords
                .iter()
                .all(|kw| ep.path.to_lowercase().contains(&kw.to_lowercase()))
            {
                return true;
            }
        }
        false
    }

    pub fn log_result(result: &ContractGateResult, log_path: &Path) {
        let mut log_entry = String::new();
        log_entry.push_str(&format!(
            "=== Contract Gate: {} ===\n", result.target
        ));
        log_entry.push_str(&format!(
            "Result: {}\n",
            if result.passed { "PASS" } else { "REJECT" }
        ));
        log_entry.push_str(&format!(
            "Contract groups: {}\n",
            result.contract_groups.join(", ")
        ));
        log_entry.push_str(&format!(
            "Core CRUD coverage: {:.1}% ({}/{})\n",
            result.coverage_pct,
            result.covered_endpoints.len(),
            result.total_core_endpoints
        ));

        if !result.missing_endpoints.is_empty() {
            log_entry.push_str("Missing endpoints:\n");
            for ep in &result.missing_endpoints {
                log_entry.push_str(&format!("  - {}\n", ep));
            }
        }

        if !result.covered_endpoints.is_empty() {
            log_entry.push_str("Covered endpoints:\n");
            for ep in &result.covered_endpoints {
                log_entry.push_str(&format!("  - {}\n", ep));
            }
        }

        if !result.admin_endpoints.is_empty() {
            log_entry.push_str(&format!(
                "Admin endpoints (excluded, {} total):\n",
                result.admin_endpoints.len()
            ));
            for ep in &result.admin_endpoints {
                log_entry.push_str(&format!("  - {}\n", ep));
            }
        }

        log_entry.push_str("\n");

        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(log_entry.as_bytes()) {
                    warn!("Failed to write contract gate log: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to open contract gate log: {}", e);
            }
        }

        let threshold = GATE_THRESHOLD;

        if result.passed {
            info!(
                "Contract gate PASSED for {}: {:.1}% coverage ({}/{}) (threshold: {:.0}%)",
                result.target,
                result.coverage_pct,
                result.covered_endpoints.len(),
                result.total_core_endpoints,
                threshold,
            );
        } else {
            warn!(
                "Contract gate REJECTED for {}: {:.1}% coverage ({}/{}) (threshold: {:.0}%), missing: {:?}",
                result.target,
                result.coverage_pct,
                result.covered_endpoints.len(),
                result.total_core_endpoints,
                threshold,
                result.missing_endpoints
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::vdbfuzz::coverage::ApiEndpoint;

    #[test]
    fn test_is_core_crud_entities() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/entities/search".into(),
            params: vec![],
        };
        assert!(ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_is_core_crud_collections() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/collections/create".into(),
            params: vec![],
        };
        assert!(ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_is_core_crud_admin_indexes() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/indexes/create".into(),
            params: vec![],
        };
        assert!(!ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_is_core_crud_admin_partitions() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/partitions/create".into(),
            params: vec![],
        };
        assert!(!ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_is_core_crud_admin_load() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/collections/load".into(),
            params: vec![],
        };
        assert!(!ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_is_core_crud_admin_meta() {
        let ep = ApiEndpoint {
            method: "GET".into(),
            path: "/v1/meta".into(),
            params: vec![],
        };
        assert!(!ContractGate::is_core_crud(&ep));
    }

    #[test]
    fn test_endpoint_covered_search_group() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/entities/search".into(),
            params: vec![],
        };
        let groups = vec!["search".to_string(), "create_collection".to_string()];
        assert!(ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_endpoint_covered_create_collection_group() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/collections/create".into(),
            params: vec![],
        };
        let groups = vec!["search".to_string(), "create_collection".to_string()];
        assert!(ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_endpoint_not_covered() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/entities/query".into(),
            params: vec![],
        };
        let groups = vec!["search".to_string(), "create_collection".to_string()];
        assert!(!ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_endpoint_covered_schema_group() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v1/schema".into(),
            params: vec![],
        };
        let groups = vec!["schema".to_string(), "objects".to_string()];
        assert!(ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_endpoint_covered_objects_group() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v1/objects".into(),
            params: vec![],
        };
        let groups = vec!["schema".to_string(), "objects".to_string()];
        assert!(ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_empty_contract_groups() {
        let ep = ApiEndpoint {
            method: "POST".into(),
            path: "/v2/vectordb/entities/search".into(),
            params: vec![],
        };
        let groups: Vec<String> = vec![];
        assert!(!ContractGate::is_endpoint_covered_by_groups(&ep, &groups));
    }

    #[test]
    fn test_empty_endpoints_trivial_pass() {
        let contract = StructuredContract {
            api_endpoint: "sql".to_string(),
            doc_url: "".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let plugin = crate::target::PgVectorPlugin;
        let result = ContractGate::check(&contract, &plugin, "pgvector");
        assert!(result.passed);
        assert_eq!(result.total_core_endpoints, 0);
        assert_eq!(result.coverage_pct, 100.0);
    }
}