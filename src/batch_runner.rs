use crate::infra;
use crate::target::TargetRegistry;
use tracing::{info, warn};

/// Resolve `{{TESTVDB_DB_URL}}` template in probe scripts.
fn resolve_db_url(script: &str, db_url: &str) -> String {
    script
        .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
        .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
        .replace("{TESTVDB_DB_URL}", db_url)
        .replace("{{TESTVDB_DB_URL}}", db_url)
}

pub async fn run_batch(
    target: &str,
    network: &Option<String>,
    db_host: &Option<String>,
    db_port: u16,
    non_redundant_only: bool,
) -> anyhow::Result<()> {
    let registry = TargetRegistry::new_with_all();
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}. Available: {:?}", target, registry.available_targets()))?;

    let pip_packages = plugin.pip_packages();
    let nets = plugin.safety_nets();
    let nets: Vec<_> = if non_redundant_only {
        nets.into_iter().filter(|n| !n.redundant_with_mutation).collect()
    } else {
        nets.into_iter().collect()
    };

    let net_name = match network {
        Some(n) => n.clone(),
        None => infra::find_docker_network(target)?,
    };
    info!("Using Docker network: {}", net_name);

    let hostname = match db_host {
        Some(h) => h.clone(),
        None => format!("{}-standalone", target),
    };
    let db_url = format!("http://{}:{}", hostname, db_port);
    info!("DB URL inside Docker: {}", db_url);

    let runner_name = format!("testvdb-batch-{}", target);
    infra::ensure_runner_container(&runner_name, &net_name, &pip_packages)?;

    let mut passed = 0usize;
    let mut defects = Vec::new();
    let mut errors = Vec::new();

    info!("Running {} safety net probes...", nets.len());

    for (i, net) in nets.iter().enumerate() {
        let script = resolve_db_url(&net.script, &db_url);

        match infra::execute_probe_script(&runner_name, &script) {
            Ok((stdout, stderr, has_defect, exit_ok)) => {
                if has_defect {
                    let defect_line = stdout.lines().chain(stderr.lines()).find(|l| l.contains("[DEFECT:")).unwrap_or("").trim().to_string();
                    warn!("[{}/{}] {} => DEFECT: {}", i + 1, nets.len(), net.name, defect_line);
                    defects.push((net.name.clone(), defect_line));
                } else if exit_ok {
                    info!("[{}/{}] {} => PASSED", i + 1, nets.len(), net.name);
                    passed += 1;
                } else {
                    let err_short = if !stderr.trim().is_empty() { &stderr[..stderr.len().min(200)] } else { &stdout[..stdout.len().min(200)] };
                    warn!("[{}/{}] {} => ERROR: {}", i + 1, nets.len(), net.name, err_short.trim());
                    errors.push((net.name.clone(), err_short.trim().to_string()));
                }
            }
            Err(e) => {
                errors.push((net.name.clone(), format!("exec_failed: {}", e)));
            }
        }
    }

    println!("\n=== SAFETY NET BATCH RESULTS ===");
    println!("Total: {} | Passed: {} | Defects: {} | Errors: {}", nets.len(), passed, defects.len(), errors.len());

    if !defects.is_empty() {
        let mut seen_defects = std::collections::HashSet::new();
        let mut unique_defects = Vec::new();
        for (name, line) in &defects {
            let key = line.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");
            if seen_defects.insert(key) {
                unique_defects.push((name.clone(), line.clone()));
            }
        }
        println!("\n--- DEFECTS FOUND ({} unique, {} total) ---", unique_defects.len(), defects.len());
        for (name, line) in &unique_defects {
            println!("  [{}] {}", name, line);
        }
        defects = unique_defects;
    }

    let baseline_path = std::path::Path::new("testvdb_baseline.json");
    let current_result = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "target": target,
        "total": nets.len(),
        "passed": passed,
        "defects": defects.iter().map(|(n, l)| serde_json::json!({"name": n, "line": l})).collect::<Vec<_>>(),
        "errors": errors.len(),
    });
    if baseline_path.exists() {
        if let Ok(prev_str) = std::fs::read_to_string(baseline_path) {
            if let Ok(prev) = serde_json::from_str::<serde_json::Value>(&prev_str) {
                let prev_defects: Vec<String> = prev["defects"].as_array().map(|a| a.iter().filter_map(|d| d["line"].as_str().map(String::from)).collect()).unwrap_or_default();
                let curr_defects: Vec<String> = defects.iter().map(|(_, l)| l.clone()).collect();
                let prev_set: std::collections::HashSet<_> = prev_defects.iter().map(|s| s.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "")).collect();
                let curr_set: std::collections::HashSet<_> = curr_defects.iter().map(|s| s.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "")).collect();
                let new_defects: Vec<_> = curr_set.difference(&prev_set).collect();
                let fixed_defects: Vec<_> = prev_set.difference(&curr_set).collect();
                if !new_defects.is_empty() || !fixed_defects.is_empty() {
                    println!("\n--- BASELINE COMPARISON ---");
                    if !new_defects.is_empty() { println!("  NEW defects: {}", new_defects.len()); }
                    if !fixed_defects.is_empty() { println!("  FIXED defects: {}", fixed_defects.len()); }
                }
            }
        }
    }
    if let Ok(json_str) = serde_json::to_string_pretty(&current_result) {
        let _ = std::fs::write(baseline_path, json_str);
    }

    if !errors.is_empty() {
        println!("\n--- ERRORS (first 20) ---");
        for (name, err) in errors.iter().take(20) {
            println!("  [{}] {}", name, &err[..err.len().min(150)]);
        }
    }

    Ok(())
}

pub async fn run_batch_simple(target: &str) -> anyhow::Result<usize> {
    let registry = TargetRegistry::new_with_all();
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}", target))?;

    let nets = plugin.safety_nets();
    let nets: Vec<_> = nets.into_iter().filter(|n| !n.redundant_with_mutation).collect();

    let hostname = format!("{}-standalone", target);
    let db_url = format!("http://{}:{}", hostname, plugin.db_port());

    let net_name = infra::find_docker_network(target)?;

    let runner_name = format!("testvdb-batch-{}", target);
    let pip_packages = plugin.pip_packages();
    infra::ensure_runner_container(&runner_name, &net_name, &pip_packages)?;

    let mut defects = 0usize;

    for net in nets.iter() {
        let script = resolve_db_url(&net.script, &db_url);

        match infra::execute_probe_script(&runner_name, &script) {
            Ok((_, _, has_defect, _)) => {
                if has_defect {
                    defects += 1;
                }
            }
            Err(_) => continue,
        }
    }

    infra::cleanup_runner(&runner_name);

    Ok(defects)
}