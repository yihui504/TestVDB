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
use tracing::{info, warn};

pub async fn run_deterministic_round(
    target: &str,
    store: &ContractStore,
    style: TargetStyle,
    feedback_sandbox: Option<&Sandbox>,
) -> Vec<BatchDefect> {
    let mut all_defects = Vec::new();

    let boundary_cases = BoundaryValueGenerator::from_store(store, style);
    if !boundary_cases.is_empty() {
        info!("Round: {} boundary cases", boundary_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "boundary", &boundary_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "boundary", &boundary_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Boundary: {} / {} defects", defects.len(), boundary_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Boundary batch failed: {} — is the DB container running?", e),
        }
    }

    let mutation_cases = MutationTestGenerator::from_store(store, style);
    if !mutation_cases.is_empty() {
        info!("Round: {} mutation cases", mutation_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "mutation", &mutation_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), Some(c.param_name.clone()))).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "mutation", &mutation_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), Some(c.param_name.clone()))).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Mutation: {} / {} defects", defects.len(), mutation_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Mutation batch failed: {} — is the DB container running?", e),
        }
    }

    let state_cases = StateTestGenerator::from_store(store, style);
    if !state_cases.is_empty() {
        info!("Round: {} state cases", state_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "state", &state_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "state", &state_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("State: {} / {} defects", defects.len(), state_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("State batch failed: {} — is the DB container running?", e),
        }
    }

    let meta_cases = MetamorphicTestGenerator::from_store(store, style);
    if !meta_cases.is_empty() {
        info!("Round: {} metamorphic cases", meta_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "meta", &meta_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "meta", &meta_cases.iter().map(|c| (c.name.clone(), c.script.clone(), Some(c.endpoint.clone()), None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Metamorphic: {} / {} defects", defects.len(), meta_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Metamorphic batch failed: {} — is the DB container running?", e),
        }
    }

    let seq_cases = SequenceTestGenerator::from_store(store, style);
    if !seq_cases.is_empty() {
        info!("Round: {} sequence cases", seq_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "seq", &seq_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "seq", &seq_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Sequence: {} / {} defects", defects.len(), seq_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Sequence batch failed: {} — is the DB container running?", e),
        }
    }

    let res_cases = ResourceTestGenerator::from_store(store, style);
    if !res_cases.is_empty() {
        info!("Round: {} resource cases", res_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "res", &res_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "res", &res_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Resource: {} / {} defects", defects.len(), res_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Resource batch failed: {} — is the DB container running?", e),
        }
    }

    let combo_cases = ComboTestGenerator::from_store(store, style);
    if !combo_cases.is_empty() {
        info!("Round: {} combo cases", combo_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "combo", &combo_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "combo", &combo_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Combo: {} / {} defects", defects.len(), combo_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Combo batch failed: {} — is the DB container running?", e),
        }
    }

    let diff_cases = DiffTestGenerator::from_store(store, style);
    if !diff_cases.is_empty() {
        info!("Round: {} differential cases", diff_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "diff", &diff_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "diff", &diff_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Differential: {} / {} defects", defects.len(), diff_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Differential batch failed: {} — is the DB container running?", e),
        }
    }

    let conc_cases = ConcurrentTestGenerator::from_store(store, style);
    if !conc_cases.is_empty() {
        info!("Round: {} concurrent cases", conc_cases.len());
        let result = match feedback_sandbox {
            Some(sb) => infra::run_generic_batch_with_sandbox(target, "conc", &conc_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>(), sb).await,
            None => infra::run_generic_batch(target, "conc", &conc_cases.iter().map(|c| (c.name.clone(), c.script.clone(), None, None)).collect::<Vec<_>>()).await,
        };
        match result {
            Ok(defects) => {
                info!("Concurrent: {} / {} defects", defects.len(), conc_cases.len());
                all_defects.extend(defects);
            }
            Err(e) => warn!("Concurrent batch failed: {} — is the DB container running?", e),
        }
    }

    all_defects
}
