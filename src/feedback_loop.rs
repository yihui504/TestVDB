use crate::contract::analyzer::BatchDefect;
use crate::contract::store::ContractStore;
use crate::infra;
use crate::sandbox::manager::Sandbox;
use crate::target::TargetStyle;
use crate::agent::vdbfuzz::boundary::BoundaryValueGenerator;
use crate::agent::vdbfuzz::mutation::MutationTestGenerator;
use crate::agent::vdbfuzz::state_gen::StateTestGenerator;
use crate::agent::vdbfuzz::metamorphic::MetamorphicTestGenerator;
use crate::agent::vdbfuzz::sequence_gen::SequenceTestGenerator;
use crate::agent::vdbfuzz::resource_combo::{ResourceTestGenerator, ComboTestGenerator};
use crate::agent::vdbfuzz::diff_concurrent::{DiffTestGenerator, ConcurrentTestGenerator};
use crate::agent::vdbfuzz::semantic::{ConcurrentStateGenerator, SemanticDriftGenerator, ResourceBoundaryGenerator};
use tracing::{info, warn};

/// Shared dispatch for deterministic generators: map cases → batch items → run → collect defects.
async fn run_one_generator<T>(
    target: &str,
    prefix: &str,
    label: &str,
    cases: Vec<T>,
    feedback_sandbox: Option<&Sandbox>,
    map_item: impl Fn(&T) -> (String, String, Option<String>, Option<String>),
    all_defects: &mut Vec<BatchDefect>,
) {
    if cases.is_empty() {
        return;
    }
    info!("Round: {} {} cases", cases.len(), label);
    let items: Vec<_> = cases.iter().map(map_item).collect();
    let result = match feedback_sandbox {
        Some(sb) => infra::run_generic_batch_with_sandbox(target, prefix, &items, sb).await,
        None => infra::run_generic_batch(target, prefix, &items).await,
    };
    match result {
        Ok(defects) => {
            info!("{}: {} / {} defects", label, defects.len(), cases.len());
            all_defects.extend(defects);
        }
        Err(e) => warn!("{} batch failed: {} — is the DB container running?", label, e),
    }
}

/// Determine whether low-yield generators should be skipped based on constraint count vs threshold.
/// Returns true when threshold > 0 and constraint count is below threshold.
pub fn should_skip_low_yield(constraint_count: usize, strategy_threshold: usize) -> bool {
    strategy_threshold > 0 && constraint_count < strategy_threshold
}

pub async fn run_deterministic_round(
    target: &str,
    store: &ContractStore,
    style: TargetStyle,
    strategy_threshold: usize,
    feedback_sandbox: Option<&Sandbox>,
) -> Vec<BatchDefect> {
    let constraint_count = store.type_constraints.len() + store.range_constraints.len();
    let skip_low_yield = should_skip_low_yield(constraint_count, strategy_threshold);
    if skip_low_yield {
        info!(
            "Strategy threshold: constraints={} < threshold={}, skipping low-yield strategies (state/meta/seq/res/combo/conc)",
            constraint_count, strategy_threshold
        );
    }
    let mut all_defects = Vec::new();

    // ── Core generators (always run) ──
    run_one_generator(target, "boundary", "Boundary",
        BoundaryValueGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(),
             c.coverage_entry.as_ref().map(|(ep, _, _)| ep.clone()),
             c.coverage_entry.as_ref().map(|(_, pn, _)| pn.clone())),
        &mut all_defects,
    ).await;

    run_one_generator(target, "mutation", "Mutation",
        MutationTestGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), Some(c.param_name.clone())),
        &mut all_defects,
    ).await;

    // ── Differential (always run) ──
    run_one_generator(target, "diff", "Differential",
        DiffTestGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
        &mut all_defects,
    ).await;

    // ── Low-yield generators (skip when constraints below threshold) ──
    if !skip_low_yield {
        run_one_generator(target, "state", "State",
            StateTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "meta", "Metamorphic",
            MetamorphicTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "seq", "Sequence",
            SequenceTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), None, None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "res", "Resource",
            ResourceTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), None, None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "combo", "Combo",
            ComboTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), None, None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "conc", "Concurrent",
            ConcurrentTestGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(), None, None),
            &mut all_defects,
        ).await;

        run_one_generator(target, "conc_state", "ConcurrentState",
            ConcurrentStateGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(),
                 c.coverage_entry.as_ref().map(|(ep, _, _)| ep.clone()),
                 c.coverage_entry.as_ref().map(|(_, pn, _)| pn.clone())),
            &mut all_defects,
        ).await;

        run_one_generator(target, "semantic", "SemanticDrift",
            SemanticDriftGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(),
                 c.coverage_entry.as_ref().map(|(ep, _, _)| ep.clone()),
                 c.coverage_entry.as_ref().map(|(_, pn, _)| pn.clone())),
            &mut all_defects,
        ).await;

        run_one_generator(target, "resource_b", "ResourceBoundary",
            ResourceBoundaryGenerator::from_store(store, style),
            feedback_sandbox,
            |c| (c.name.clone(), c.script.clone(),
                 c.coverage_entry.as_ref().map(|(ep, _, _)| ep.clone()),
                 c.coverage_entry.as_ref().map(|(_, pn, _)| pn.clone())),
            &mut all_defects,
        ).await;
    }

    all_defects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::store::{
        AnnotatedRangeConstraint, AnnotatedTypeConstraint, Confidence, ConstraintSource,
        ObservedBehavior,
    };
    use crate::contract::schema::{RangeConstraint, RejectionPolicy, TypeConstraint};

    // ── should_skip_low_yield tests ──

    #[test]
    fn test_skip_low_yield_threshold_zero_never_skip() {
        assert!(!should_skip_low_yield(0, 0));
        assert!(!should_skip_low_yield(5, 0));
        assert!(!should_skip_low_yield(100, 0));
    }

    #[test]
    fn test_skip_low_yield_below_threshold() {
        assert!(should_skip_low_yield(0, 10));
        assert!(should_skip_low_yield(5, 10));
        assert!(should_skip_low_yield(9, 10));
    }

    #[test]
    fn test_skip_low_yield_at_or_above_threshold() {
        assert!(!should_skip_low_yield(10, 10));
        assert!(!should_skip_low_yield(15, 10));
        assert!(!should_skip_low_yield(100, 10));
    }

    // ── ContractStore interaction tests (feedback loop data flow) ──

    fn make_type_constraint(param_name: &str, endpoint: &str) -> AnnotatedTypeConstraint {
        AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: param_name.to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some(endpoint.to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        }
    }

    fn make_range_constraint(param_name: &str, endpoint: &str, min: f64, max: f64) -> AnnotatedRangeConstraint {
        AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: param_name.to_string(),
                description: format!("{} range", param_name),
                min: Some(min),
                max: Some(max),
                violation_examples: vec![],
            },
            endpoint: Some(endpoint.to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        }
    }

    #[test]
    fn test_store_constraint_count_for_skip_decision() {
        let mut store = ContractStore::new("qdrant", "v1.13");
        // Empty store: 0 constraints
        assert_eq!(store.type_constraints.len() + store.range_constraints.len(), 0);
        assert!(should_skip_low_yield(0, 5));

        store.type_constraints.push(make_type_constraint("limit", "/points/search"));
        assert_eq!(store.type_constraints.len() + store.range_constraints.len(), 1);
        assert!(should_skip_low_yield(1, 5));

        store.range_constraints.push(make_range_constraint("limit", "/points/search", 1.0, 16384.0));
        assert_eq!(store.type_constraints.len() + store.range_constraints.len(), 2);
        assert!(should_skip_low_yield(2, 5));

        // Add more to reach threshold
        for i in 0..3 {
            store.type_constraints.push(make_type_constraint(
                &format!("param_{}", i), "/test",
            ));
        }
        assert_eq!(store.type_constraints.len() + store.range_constraints.len(), 5);
        assert!(!should_skip_low_yield(5, 5));
    }

    #[test]
    fn test_assimilate_violation_adds_type_constraint() {
        let mut store = ContractStore::new("qdrant", "v1.13");
        assert!(store.type_constraints.is_empty());
        assert!(store.observed_behaviors.is_empty());

        store.assimilate_observation(ObservedBehavior {
            endpoint: "/points/search".to_string(),
            param_name: "limit".to_string(),
            description: "limit=null accepted".to_string(),
            observed_value: "null".to_string(),
            expected_behavior: "reject null".to_string(),
            actual_behavior: "accepted null".to_string(),
            is_violation: true,
        });

        assert_eq!(store.type_constraints.len(), 1);
        assert_eq!(store.observed_behaviors.len(), 1);
        let tc = &store.type_constraints[0];
        assert!(matches!(tc.source, ConstraintSource::ObservedBehavior));
        assert!(matches!(tc.confidence, Confidence::High));
        assert_eq!(tc.rejection_policy, Some(RejectionPolicy::Reject));
    }

    #[test]
    fn test_assimilate_non_violation_no_type_constraint() {
        let mut store = ContractStore::new("qdrant", "v1.13");

        store.assimilate_observation(ObservedBehavior {
            endpoint: "/points/search".to_string(),
            param_name: "limit".to_string(),
            description: "limit=10 works".to_string(),
            observed_value: "10".to_string(),
            expected_behavior: "accept integer".to_string(),
            actual_behavior: "accepted".to_string(),
            is_violation: false,
        });

        assert!(store.type_constraints.is_empty());
        assert_eq!(store.observed_behaviors.len(), 1);
    }

    #[test]
    fn test_assimilate_multiple_observations_accumulate() {
        let mut store = ContractStore::new("qdrant", "v1.13");

        for i in 0..3 {
            store.assimilate_observation(ObservedBehavior {
                endpoint: format!("/ep{}", i),
                param_name: format!("param{}", i),
                description: format!("obs{}", i),
                observed_value: "null".to_string(),
                expected_behavior: "reject".to_string(),
                actual_behavior: "accepted".to_string(),
                is_violation: true,
            });
        }

        assert_eq!(store.type_constraints.len(), 3);
        assert_eq!(store.observed_behaviors.len(), 3);
    }

    #[test]
    fn test_merge_dedup_observed_behaviors() {
        let mut store1 = ContractStore::new("qdrant", "v1.13");
        let obs = ObservedBehavior {
            endpoint: "/points/search".to_string(),
            param_name: "limit".to_string(),
            description: "limit=null accepted".to_string(),
            observed_value: "null".to_string(),
            expected_behavior: "reject".to_string(),
            actual_behavior: "accepted".to_string(),
            is_violation: true,
        };
        store1.assimilate_observation(obs.clone());

        let mut store2 = ContractStore::new("qdrant", "v1.13");
        store2.assimilate_observation(obs.clone());

        store1.merge(store2);
        assert_eq!(store1.observed_behaviors.len(), 1, "duplicate observation should be deduped");
    }

    #[test]
    fn test_merge_type_constraint_rejection_policy_upgrade() {
        let mut store1 = ContractStore::new("qdrant", "v1.13");
        store1.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/points/search".to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::Medium,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });

        let mut store2 = ContractStore::new("qdrant", "v1.13");
        store2.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/points/search".to_string()),
            source: ConstraintSource::ObservedBehavior,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store1.merge(store2);
        assert_eq!(store1.type_constraints.len(), 1, "same endpoint+param should dedup");
        assert_eq!(store1.type_constraints[0].rejection_policy, Some(RejectionPolicy::Reject),
            "Ignore should be upgraded to Reject on merge");
    }

    #[test]
    fn test_merge_range_constraint_rejection_policy_upgrade() {
        let mut store1 = ContractStore::new("qdrant", "v1.13");
        store1.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit range".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("/points/search".to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::Medium,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });

        let mut store2 = ContractStore::new("qdrant", "v1.13");
        store2.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit range".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("/points/search".to_string()),
            source: ConstraintSource::ObservedBehavior,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store1.merge(store2);
        assert_eq!(store1.range_constraints.len(), 1, "same endpoint+param should dedup");
        assert_eq!(store1.range_constraints[0].rejection_policy, Some(RejectionPolicy::Reject),
            "Ignore should be upgraded to Reject on merge");
    }
}
