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

pub async fn run_deterministic_round(
    target: &str,
    store: &ContractStore,
    style: TargetStyle,
    feedback_sandbox: Option<&Sandbox>,
) -> Vec<BatchDefect> {
    let mut all_defects = Vec::new();

    // ── Core generators ──
    run_one_generator(target, "boundary", "Boundary",
        BoundaryValueGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
        &mut all_defects,
    ).await;

    run_one_generator(target, "mutation", "Mutation",
        MutationTestGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), Some(c.param_name.clone())),
        &mut all_defects,
    ).await;

    run_one_generator(target, "state", "State",
        StateTestGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None),
        &mut all_defects,
    ).await;

    run_one_generator(target, "meta", "Metamorphic",
        MetamorphicTestGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
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

    // ── Differential & concurrent ──
    run_one_generator(target, "diff", "Differential",
        DiffTestGenerator::from_store(store, style),
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

    // ── Semantic / state / resource generators ──
    run_one_generator(target, "conc_state", "ConcurrentState",
        ConcurrentStateGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
        &mut all_defects,
    ).await;

    run_one_generator(target, "semantic", "SemanticDrift",
        SemanticDriftGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
        &mut all_defects,
    ).await;

    run_one_generator(target, "resource", "ResourceBoundary",
        ResourceBoundaryGenerator::from_store(store, style),
        feedback_sandbox,
        |c| (c.name.clone(), c.script.clone(), None, None),
        &mut all_defects,
    ).await;

    all_defects
}
