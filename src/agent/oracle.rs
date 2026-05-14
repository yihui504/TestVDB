use crate::agent::classifier::{detect_defect_type, is_script_error, DefectType};
use crate::contract::schema::{CheckType, MutationRule, StateInvariant};
use crate::sandbox::manager::Sandbox;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantCheck {
    pub name: String,
    pub check_type: CheckType,
    pub script: String,
    pub source: InvariantSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InvariantSource {
    ContractExplicit,
    DerivedFromAssertion,
    DerivedFromRange,
    DerivedFromState,
    DerivedFromType,
    DerivedFromBehavior,
    DerivedFromMutation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleFinding {
    pub invariant_name: String,
    pub violated: bool,
    pub evidence: String,
    pub defect_type: Option<DefectType>,
}

pub struct Oracle {
    checks: Vec<InvariantCheck>,
    checks_run: usize,
}

impl Oracle {
    pub fn new(checks: Vec<InvariantCheck>) -> Self {
        info!("Oracle initialized with {} invariant checks", checks.len());
        Oracle {
            checks,
            checks_run: 0,
        }
    }

    pub fn from_explicit_invariants(invariants: &[StateInvariant]) -> Vec<InvariantCheck> {
        invariants
            .iter()
            .map(|si| InvariantCheck {
                name: si.name.clone(),
                check_type: si.check_type.clone(),
                script: si.assertion_script.clone(),
                source: InvariantSource::ContractExplicit,
            })
            .collect()
    }

    pub fn from_mutation_rules(rules: &[MutationRule]) -> Vec<InvariantCheck> {
        rules
            .iter()
            .map(|rule| InvariantCheck {
                name: format!("mutation_{}", rule.name),
                check_type: CheckType::ValueRange,
                script: rule.mutated_script.clone(),
                source: InvariantSource::DerivedFromMutation,
            })
            .collect()
    }

    pub fn pending_checks(&self) -> &[InvariantCheck] {
        &self.checks[self.checks_run..]
    }

    pub fn has_pending(&self) -> bool {
        self.checks_run < self.checks.len()
    }

    pub async fn run_next_batch(
        &mut self,
        sandbox: &Sandbox,
        db_url: &str,
        batch_size: usize,
    ) -> Vec<OracleFinding> {
        let mut findings = Vec::new();
        let end = usize::min(self.checks_run + batch_size, self.checks.len());

        for i in self.checks_run..end {
            let check = &self.checks[i];
            info!("Oracle running check: {} ({:?})", check.name, check.check_type);

            let script = check
                .script
                .replace("{TESTVDB_DB_URL}", db_url)
                .replace("{{TESTVDB_DB_URL}}", db_url);
            let script_b64 = base64::engine::general_purpose::STANDARD.encode(&script);
            let script_path = format!("/tmp/oracle_{}.py", check.name);
            let decode_script = format!(
                "import base64\nopen('{}','wb').write(base64.b64decode('{}'))",
                script_path, script_b64
            );
            let write_result = sandbox
                .exec_script(&decode_script, &[])
                .await;

            let result = match write_result {
                Ok(_) => {
                    sandbox
                        .exec_command_with_env(&["python", &script_path], &[("TESTVDB_DB_URL", db_url)])
                        .await
                }
                Err(e) => Err(e),
            };

            match result {
                Ok(output) => {
                    let stdout = output.stdout.trim().to_string();
                    let stderr = output.stderr.trim().to_string();

                    if !stdout.is_empty() {
                        debug!("Oracle check '{}' stdout: {}", check.name, &stdout[..stdout.len().min(500)]);
                    }
                    if !stderr.is_empty() {
                        debug!("Oracle check '{}' stderr: {}", check.name, &stderr[..stderr.len().min(500)]);
                    }

                    let finding = Self::classify_check_result(check, &stdout, &stderr);
                    if finding.violated {
                        warn!(
                            "Oracle FOUND violation: {} — {}",
                            finding.invariant_name, finding.evidence
                        );
                    } else {
                        info!(
                            "Oracle check passed: {} — stdout: {}",
                            check.name,
                            if stdout.is_empty() { "(empty)" } else { &stdout[..stdout.len().min(200)] }
                        );
                    }
                    findings.push(finding);
                }
                Err(e) => {
                    warn!("Oracle check '{}' execution failed: {}", check.name, e);
                    findings.push(OracleFinding {
                        invariant_name: check.name.clone(),
                        violated: false,
                        evidence: format!("Execution error: {}", e),
                        defect_type: None,
                    });
                }
            }
        }

        self.checks_run = end;
        findings
    }

    pub async fn run_all_pending(
        &mut self,
        sandbox: &Sandbox,
        db_url: &str,
    ) -> Vec<OracleFinding> {
        let remaining = self.checks.len() - self.checks_run;
        self.run_next_batch(sandbox, db_url, remaining).await
    }

    fn classify_check_result(
        check: &InvariantCheck,
        stdout: &str,
        stderr: &str,
    ) -> OracleFinding {
        if is_script_error(stdout, stderr) || stderr.to_lowercase().contains("traceback") {
            return OracleFinding {
                invariant_name: check.name.clone(),
                violated: false,
                evidence: format!("Script error (not a defect): {}", &stderr[..stderr.len().min(200)]),
                defect_type: None,
            };
        }

        if let Some(defect_type) = detect_defect_type(stdout, stderr) {
            let evidence = if !stdout.is_empty() {
                stdout.chars().take(500).collect()
            } else {
                stderr.chars().take(500).collect()
            };

            return OracleFinding {
                invariant_name: check.name.clone(),
                violated: true,
                evidence,
                defect_type: Some(defect_type),
            };
        }

        if !stderr.is_empty() && stdout.is_empty() {
            warn!("Oracle check '{}' had stderr but no stdout: {}", check.name, &stderr[..stderr.len().min(300)]);
        }

        OracleFinding {
            invariant_name: check.name.clone(),
            violated: false,
            evidence: if stdout.is_empty() {
                "Check passed — invariant held.".to_string()
            } else {
                stdout.chars().take(300).collect()
            },
            defect_type: None,
        }
    }

    pub fn total_checks(&self) -> usize {
        self.checks.len()
    }

    pub fn checks_completed(&self) -> usize {
        self.checks_run
    }
}

pub fn build_oracle_findings_message(findings: &[OracleFinding]) -> String {
    if findings.is_empty() {
        return String::new();
    }

    let violated: Vec<&OracleFinding> = findings.iter().filter(|f| f.violated).collect();
    let passed: Vec<&OracleFinding> = findings.iter().filter(|f| !f.violated).collect();

    let mut msg = String::from("=== ORACLE FINDINGS ===\n");

    if !violated.is_empty() {
        msg.push_str(&format!("VIOLATIONS DETECTED: {}\n", violated.len()));
        for f in &violated {
            msg.push_str(&format!(
                "- {} [{:?}]: {}\n",
                f.invariant_name,
                f.defect_type.as_ref().unwrap_or(&DefectType::Pass),
                f.evidence.chars().take(500).collect::<String>()
            ));
        }
    }

    if !passed.is_empty() {
        msg.push_str(&format!("PASSED: {} checks\n", passed.len()));
    }

    msg.push_str("=== END ORACLE ===\n");

    if !violated.is_empty() {
        msg.push_str(
            "\nThe Oracle detected potential defects! Here is how to reproduce them:\n\
             1. Create a collection with vectors (size=4, distance=Cosine)\n\
             2. Insert test points into the collection\n\
             3. Perform the operation that triggered the violation\n\
             4. Check if the result matches the expected outcome\n\
             Write a COMPLETE Python script that does ALL steps in one execute_test_script call.\n\
             Use {{TESTVDB_DB_URL}} as the database URL placeholder.\n\
             Print [DEFECT: STATE_LOGIC_VIOLATION] or [DEFECT: ILLEGAL_SUCCESS] if the defect is confirmed.",
        );
    }

    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_illegal_success() {
        let check = InvariantCheck {
            name: "limit_zero".to_string(),
            check_type: CheckType::ValueRange,
            script: String::new(),
            source: InvariantSource::DerivedFromRange,
        };
        let finding = Oracle::classify_check_result(
            &check,
            "[DEFECT: ILLEGAL_SUCCESS] limit=0 accepted",
            "",
        );
        assert!(finding.violated);
        assert_eq!(finding.defect_type, Some(DefectType::IllegalSuccess));
    }

    #[test]
    fn test_classify_state_violation() {
        let check = InvariantCheck {
            name: "count_consistency".to_string(),
            check_type: CheckType::CountConsistency,
            script: String::new(),
            source: InvariantSource::DerivedFromAssertion,
        };
        let finding = Oracle::classify_check_result(
            &check,
            "",
            "AssertionError: count mismatch: expected 5, got 0",
        );
        assert!(finding.violated);
        assert_eq!(finding.defect_type, Some(DefectType::StateLogicViolation));
    }

    #[test]
    fn test_classify_pass() {
        let check = InvariantCheck {
            name: "limit_zero".to_string(),
            check_type: CheckType::ValueRange,
            script: String::new(),
            source: InvariantSource::DerivedFromRange,
        };
        let finding = Oracle::classify_check_result(&check, "properly rejected limit=0: 400", "");
        assert!(!finding.violated);
        assert_eq!(finding.defect_type, None);
    }

    #[test]
    fn test_classify_script_error() {
        let check = InvariantCheck {
            name: "limit_zero".to_string(),
            check_type: CheckType::ValueRange,
            script: String::new(),
            source: InvariantSource::DerivedFromRange,
        };
        let finding = Oracle::classify_check_result(
            &check,
            "",
            "SyntaxError: invalid syntax",
        );
        assert!(!finding.violated);
    }

    #[test]
    fn test_build_oracle_findings_message_with_violation() {
        let findings = vec![
            OracleFinding {
                invariant_name: "count_check".to_string(),
                violated: true,
                evidence: "count=0 but expected 5".to_string(),
                defect_type: Some(DefectType::StateLogicViolation),
            },
            OracleFinding {
                invariant_name: "limit_check".to_string(),
                violated: false,
                evidence: "passed".to_string(),
                defect_type: None,
            },
        ];
        let msg = build_oracle_findings_message(&findings);
        assert!(msg.contains("VIOLATIONS DETECTED: 1"));
        assert!(msg.contains("count_check"));
        assert!(msg.contains("PASSED: 1"));
        assert!(msg.contains("how to reproduce"));
    }

    #[test]
    fn test_build_oracle_findings_message_empty() {
        let msg = build_oracle_findings_message(&[]);
        assert!(msg.is_empty());
    }

    #[test]
    fn test_from_explicit_invariants() {
        let invariants = vec![StateInvariant {
            name: "count_after_upsert".to_string(),
            check_type: CheckType::CountConsistency,
            endpoint: "/collections/test/points".to_string(),
            precondition: "collection exists".to_string(),
            assertion_script: "print('check')".to_string(),
        }];
        let checks = Oracle::from_explicit_invariants(&invariants);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].source, InvariantSource::ContractExplicit);
        assert_eq!(checks[0].name, "count_after_upsert");
    }

    #[test]
    fn test_oracle_batch_tracking() {
        let checks = vec![
            InvariantCheck {
                name: "a".to_string(),
                check_type: CheckType::ValueRange,
                script: "pass".to_string(),
                source: InvariantSource::DerivedFromRange,
            },
            InvariantCheck {
                name: "b".to_string(),
                check_type: CheckType::ExistenceCheck,
                script: "pass".to_string(),
                source: InvariantSource::DerivedFromState,
            },
        ];
        let oracle = Oracle::new(checks);
        assert_eq!(oracle.checks_completed(), 0);
        assert_eq!(oracle.pending_checks().len(), 2);
        assert!(oracle.has_pending());
    }
}
