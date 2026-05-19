use crate::agent::classifier::{analyze_execution_result, ClassificationDisposition, normalize_observed_output};
use crate::report::generator::{CandidateDefect, CandidateStatus, RunEvidence};
use crate::sandbox::manager::SidecarSpec;

pub async fn run_script_in_fresh_sandbox(
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    script_code: &str,
    phase: &str,
    sidecars: &[SidecarSpec],
    db_env: &[(String, String)],
    db_command: &[String],
) -> anyhow::Result<RunEvidence> {
    use crate::sandbox::manager::Sandbox;

    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port, sidecars, db_env, db_command).await?;
    let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap(), db_port);
    let rebound_script = script_code
        .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
        .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
        .replace("{TESTVDB_DB_URL}", &db_url)
        .replace("{{TESTVDB_DB_URL}}", &db_url);
    let output = sandbox.exec_script(&rebound_script, &[("TESTVDB_DB_URL", &db_url)]).await?;
    let normalized_stdout = normalize_observed_output(&output.stdout);
    let normalized_stderr = normalize_observed_output(&output.stderr);
    let classification = analyze_execution_result(&normalized_stdout, &normalized_stderr);

    Ok(RunEvidence {
        phase: phase.to_string(),
        db_url,
        stdout: normalized_stdout,
        stderr: normalized_stderr,
        classifier_reason: classification.reason,
        classifier_evidence_excerpt: classification.evidence_excerpt,
    })
}

pub async fn refresh_candidate_evidence_with_mre(
    candidate: &mut CandidateDefect,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    sidecars: &[SidecarSpec],
    db_env: &[(String, String)],
    db_command: &[String],
) -> anyhow::Result<Option<String>> {
    let phases = ["initial", "repro_1", "repro_2"];
    let mut refreshed_runs = Vec::new();
    let mut expected_excerpt: Option<String> = None;

    for phase in phases {
        let run = match run_script_in_fresh_sandbox(
            db_image,
            pip_packages,
            db_port,
            &candidate.mre_code,
            phase,
            sidecars,
            db_env,
            db_command,
        )
        .await
        {
            Ok(run) => run,
            Err(err) => {
                candidate.status = CandidateStatus::Rejected;
                candidate.downgrade_reason = Some(format!(
                    "{} could not be replayed after narrowing candidate evidence: {}",
                    phase, err
                ));
                if !refreshed_runs.is_empty() {
                    candidate.initial_run = refreshed_runs.remove(0);
                    candidate.reproduction_runs = refreshed_runs;
                }
                return Ok(candidate.downgrade_reason.clone());
            }
        };
        let classification = analyze_execution_result(&run.stdout, &run.stderr);

        if classification.disposition != ClassificationDisposition::CandidateDefect
            || classification.defect_type.as_ref() != Some(&candidate.defect_type)
        {
            candidate.status = CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(format!(
                "{} failed after narrowing candidate evidence: {}",
                phase, classification.reason
            ));
            refreshed_runs.push(run);
            candidate.initial_run = refreshed_runs.remove(0);
            candidate.reproduction_runs = refreshed_runs;
            return Ok(candidate.downgrade_reason.clone());
        }

        if let Some(expected) = &expected_excerpt {
            if classification.evidence_excerpt != *expected {
                candidate.status = CandidateStatus::Rejected;
                candidate.downgrade_reason = Some(format!(
                    "{} produced a different evidence excerpt after narrowing candidate evidence.",
                    phase
                ));
                refreshed_runs.push(run);
                candidate.initial_run = refreshed_runs.remove(0);
                candidate.reproduction_runs = refreshed_runs;
                return Ok(candidate.downgrade_reason.clone());
            }
        } else {
            expected_excerpt = Some(classification.evidence_excerpt.clone());
        }

        refreshed_runs.push(run);
    }

    candidate.initial_run = refreshed_runs.remove(0);
    candidate.reproduction_runs = refreshed_runs;

    Ok(None)
}

pub async fn run_additional_reproduction(
    candidate: &mut CandidateDefect,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    sidecars: &[SidecarSpec],
    db_env: &[(String, String)],
    db_command: &[String],
) -> anyhow::Result<Option<String>> {
    let run = match run_script_in_fresh_sandbox(
        db_image,
        pip_packages,
        db_port,
        &candidate.mre_code,
        "independent_review",
        sidecars,
        db_env,
        db_command,
    )
    .await
    {
        Ok(run) => run,
        Err(err) => {
            return Ok(Some(format!(
                "Independent replay of the stabilized MRE could not complete cleanly: {}",
                err
            )));
        }
    };
    let classification = analyze_execution_result(&run.stdout, &run.stderr);
    if classification.disposition != ClassificationDisposition::CandidateDefect
        || classification.defect_type.as_ref() != Some(&candidate.defect_type)
    {
        return Ok(Some(format!(
            "Independent replay of the stabilized MRE did not confirm the verified finding: {}",
            classification.reason
        )));
    }

    candidate.independent_review_summary = Some(format!(
        "Fresh post-verification replay of the stabilized MRE reproduced {:?} with the same evidence excerpt.",
        candidate.defect_type
    ));
    candidate.review_scope = Some(
        "Independent review reran the stabilized final MRE in a fresh sandbox after double reproduction, outside the initial promotion loop."
            .to_string(),
    );

    Ok(None)
}
