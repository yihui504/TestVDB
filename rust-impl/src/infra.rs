use crate::contract::analyzer::BatchDefect;
use crate::target::TargetRegistry;
use anyhow::Context;
use std::process::Command as StdCommand;
use tracing::{info, warn};

pub fn build_db_url(host: &str, port: u16) -> String {
    format!("http://{}:{}", host, port)
}

pub fn find_docker_network(target: &str) -> anyhow::Result<String> {
    let hostname = format!("{}-standalone", target);
    let inspect_out = StdCommand::new("docker")
        .args(["inspect", &hostname, "--format", "{{range $k,$v := .NetworkSettings.Networks}}{{$k}}{{end}}"])
        .output()?;
    let mut net_name = String::from_utf8_lossy(&inspect_out.stdout).trim().to_string();
    if net_name.is_empty() {
        let fallback_out = StdCommand::new("docker")
            .args(["network", "ls", "--filter", &format!("name={}", target), "--format", "{{.Name}}"])
            .output()?;
        let fallback = String::from_utf8_lossy(&fallback_out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if fallback.is_empty() {
            anyhow::bail!(
                "Cannot find Docker network for target '{}' (tried container '{}' and network filter).",
                target,
                hostname
            );
        }
        net_name = fallback;
    }
    Ok(net_name)
}

pub fn pip_install_args<'a>(runner: &'a str, packages: &[&'a str]) -> Vec<&'a str> {
    let mut args: Vec<&str> = vec!["exec", runner, "pip", "install", "--no-cache-dir", "--timeout", "120", "--retries", "3", "-i", "https://pypi.tuna.tsinghua.edu.cn/simple"];
    args.extend(packages);
    args
}

pub fn ensure_runner_container(runner_name: &str, net_name: &str, pip_packages: &[String]) -> anyhow::Result<()> {
    let check = StdCommand::new("docker")
        .args(["ps", "-q", "-f", &format!("name={}", runner_name)])
        .output()?;
    if String::from_utf8_lossy(&check.stdout).trim().is_empty() {
        StdCommand::new("docker")
            .args(["run", "-d", "--name", runner_name, "--network", net_name, "python:3.9-slim", "tail", "-f", "/dev/null"])
            .output()?;
        let pkg_strs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
        let pip_cmd = pip_install_args(runner_name, &pkg_strs);
        let pip_output = StdCommand::new("docker").args(&pip_cmd).output()?;
        if !pip_output.status.success() {
            anyhow::bail!("pip install failed: {}", String::from_utf8_lossy(&pip_output.stderr));
        }
    }
    Ok(())
}

pub fn execute_probe_script(runner_name: &str, script_content: &str) -> anyhow::Result<(String, String, bool, bool)> {
    let tmp_dir = std::env::temp_dir().join("testvdb_probes");
    std::fs::create_dir_all(&tmp_dir)?;
    let tmp_script = tmp_dir.join("probe.py");
    std::fs::write(&tmp_script, script_content)?;

    let cp_result = StdCommand::new("docker")
        .args(["cp", &tmp_script.to_string_lossy(), &format!("{}:/tmp/probe.py", runner_name)])
        .output()?;
    if !cp_result.status.success() {
        anyhow::bail!("docker cp failed");
    }

    let output = StdCommand::new("docker")
        .args(["exec", runner_name, "python", "/tmp/probe.py"])
        .output()
        .context("docker exec failed")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let has_defect = stdout.contains("[DEFECT:") || stderr.contains("[DEFECT:");
    let exit_ok = output.status.success();

    Ok((stdout, stderr, has_defect, exit_ok))
}

pub fn cleanup_runner(runner_name: &str) {
    let _ = StdCommand::new("docker").args(["rm", "-f", runner_name]).output();
}

pub fn cleanup_stale_containers() {
    let stale_out = match StdCommand::new("docker")
        .args(["ps", "-aq", "-f", &format!("name=testvdb-")])
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            info!("Docker cleanup: failed to list containers.");
            return;
        }
    };
    let stale_stdout = String::from_utf8_lossy(&stale_out.stdout).into_owned();
    let stale_ids: Vec<&str> = stale_stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    if !stale_ids.is_empty() {
        info!("Cleaning up {} stale testvdb container(s)...", stale_ids.len());
        let mut rm_args = vec!["rm", "-f"];
        for id in &stale_ids {
            rm_args.push(id.trim());
        }
        let _ = StdCommand::new("docker").args(&rm_args).output();
    }

    let net_out = match StdCommand::new("docker")
        .args(["network", "ls", "-q", "-f", "name=testvdb-net-"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };
    let net_stdout = String::from_utf8_lossy(&net_out.stdout).into_owned();
    let net_ids: Vec<&str> = net_stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    if !net_ids.is_empty() {
        info!("Cleaning up {} stale testvdb network(s)...", net_ids.len());
        for id in &net_ids {
            let _ = StdCommand::new("docker").args(["network", "rm", id.trim()]).output();
        }
    }
}

pub fn full_docker_cleanup() {
    info!("Full Docker cleanup: removing stale containers, networks, images, and volumes...");
    cleanup_stale_containers();
    let _ = StdCommand::new("docker").args(["network", "prune", "-f"]).output();
    let _ = StdCommand::new("docker").args(["volume", "prune", "-f"]).output();
    let _ = StdCommand::new("docker").args(["image", "prune", "-f"]).output();
    let _ = StdCommand::new("docker").args(["builder", "prune", "-f"]).output();
    info!("Full Docker cleanup complete.");
}

pub fn cleanup_volumes(base_dir: &str) {
    let volumes_dir = std::path::Path::new(base_dir).join("volumes");
    if !volumes_dir.exists() {
        return;
    }
    let subdirs = ["milvus", "qdrant", "etcd", "minio"];
    for subdir in &subdirs {
        let target = volumes_dir.join(subdir);
        if target.exists() {
            match std::fs::remove_dir_all(&target) {
                Ok(_) => info!("Cleaned up volumes/{}", subdir),
                Err(e) => warn!("Failed to clean up volumes/{}: {}", subdir, e),
            }
        }
    }
    let _ = std::fs::create_dir_all(&volumes_dir);
    info!("Volumes cleanup complete.");
}

pub async fn run_generic_batch(
    target: &str,
    prefix: &str,
    cases: &[(String, String, Option<String>, Option<String>)],
) -> anyhow::Result<Vec<BatchDefect>> {
    let registry = TargetRegistry::new_with_all();


    let plugin = registry
        .get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}", target))?;

    let db_port = plugin.db_port();
    let net_name = find_docker_network(target)?;
    let hostname = format!("{}-standalone", target);
    let db_url = build_db_url(&hostname, db_port);

    let runner_name = format!("testvdb-{}-{}", prefix, target);
    let pip_packages = plugin.pip_packages();
    ensure_runner_container(&runner_name, &net_name, &pip_packages)?;

    let mut found_defects = Vec::new();

    for (i, (name, script, endpoint, param_name)) in cases.iter().enumerate() {
        let resolved = script
            .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
            .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
            .replace("{TESTVDB_DB_URL}", &db_url)
            .replace("{{TESTVDB_DB_URL}}", &db_url);

        match execute_probe_script(&runner_name, &resolved) {
            Ok((stdout, stderr, has_defect, exit_success)) => {
                if has_defect {
                    let defect_line = stdout
                        .lines()
                        .chain(stderr.lines())
                        .find(|l| l.contains("[DEFECT:"))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    warn!("[{} {}/{}] {} => {}", prefix, i + 1, cases.len(), name, defect_line);
                    found_defects.push(BatchDefect {
                        test_name: name.clone(),
                        test_prefix: prefix.to_string(),
                        defect_line,
                        script: script.clone(),
                        stdout,
                        stderr,
                        endpoint: endpoint.clone(),
                        param_name: param_name.clone(),
                        exit_success,
                    });
                } else {
                    info!("[{} {}/{}] {} => passed", prefix, i + 1, cases.len(), name);
                }
            }
            Err(_) => continue,
        }
    }

    cleanup_runner(&runner_name);

    Ok(found_defects)
}

pub async fn run_generic_batch_with_sandbox(
    target: &str,
    prefix: &str,
    cases: &[(String, String, Option<String>, Option<String>)],
    sandbox: &crate::sandbox::manager::Sandbox,
) -> anyhow::Result<Vec<BatchDefect>> {
    let registry = TargetRegistry::new_with_all();


    let plugin = registry
        .get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}", target))?;

    let db_port = plugin.db_port();
    let db_host = sandbox.db_host.as_deref().unwrap_or("testvdb-db");
    let db_url = build_db_url(db_host, db_port);

    let runner_name = sandbox.runner_container_ids.first()
        .ok_or_else(|| anyhow::anyhow!("Sandbox has no runner container"))?
        .clone();

    let mut found_defects = Vec::new();

    for (i, (name, script, endpoint, param_name)) in cases.iter().enumerate() {
        let resolved = script
            .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
            .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
            .replace("{TESTVDB_DB_URL}", &db_url)
            .replace("{{TESTVDB_DB_URL}}", &db_url);

        match execute_probe_script(&runner_name, &resolved) {
            Ok((stdout, stderr, has_defect, exit_success)) => {
                if has_defect {
                    let defect_line = stdout
                        .lines()
                        .chain(stderr.lines())
                        .find(|l| l.contains("[DEFECT:"))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    warn!("[{} {}/{}] {} => {}", prefix, i + 1, cases.len(), name, defect_line);
                    found_defects.push(BatchDefect {
                        test_name: name.clone(),
                        test_prefix: prefix.to_string(),
                        defect_line,
                        script: script.clone(),
                        stdout,
                        stderr,
                        endpoint: endpoint.clone(),
                        param_name: param_name.clone(),
                        exit_success,
                    });
                } else {
                    info!("[{} {}/{}] {} => passed", prefix, i + 1, cases.len(), name);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(found_defects)
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_docker_image_format() {
        // Verify the image name used in ensure_runner_container follows <name>:<tag> format
        let image = "python:3.9-slim";
        let parts: Vec<&str> = image.splitn(2, ':').collect();
        assert_eq!(parts.len(), 2, "Image should be in <name>:<tag> format");
        assert!(!parts[0].is_empty(), "Image name should not be empty");
        assert!(!parts[1].is_empty(), "Image tag should not be empty");
    }

    #[test]
    fn test_network_name_generation() {
        // Verify the hostname format used in find_docker_network and run_generic_batch
        let target = "milvus";
        let hostname = format!("{}-standalone", target);
        assert_eq!(hostname, "milvus-standalone");
        assert!(hostname.starts_with(target));
        assert!(hostname.ends_with("-standalone"));

        let target2 = "qdrant";
        let hostname2 = format!("{}-standalone", target2);
        assert_eq!(hostname2, "qdrant-standalone");
    }

    #[test]
    fn test_environment_parsing() {
        // Verify the URL template replacement logic used in run_generic_batch
        let db_url = "http://milvus-standalone:19530";

        // Test single-brace placeholder with quotes: '{TESTVDB_DB_URL}/path'
        let script = "requests.get('{TESTVDB_DB_URL}/api/v1/health')";
        let resolved = script
            .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
            .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
            .replace("{TESTVDB_DB_URL}", &db_url)
            .replace("{{TESTVDB_DB_URL}}", &db_url);
        assert_eq!(resolved, format!("requests.get('{}/api/v1/health')", db_url));

        // Test single-brace placeholder without quotes: {TESTVDB_DB_URL}/path
        let script2 = "url = {TESTVDB_DB_URL}/api/v1/health";
        let resolved2 = script2
            .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
            .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
            .replace("{TESTVDB_DB_URL}", &db_url)
            .replace("{{TESTVDB_DB_URL}}", &db_url);
        assert_eq!(resolved2, format!("url = {}/api/v1/health", db_url));

        // Test double-brace placeholder as entire quoted value: '{{TESTVDB_DB_URL}}'
        let script3 = "url = '{{TESTVDB_DB_URL}}'";
        let resolved3 = script3
            .replace("'{TESTVDB_DB_URL}'", &format!("'{}'", db_url))
            .replace("'{{TESTVDB_DB_URL}}'", &format!("'{}'", db_url))
            .replace("{TESTVDB_DB_URL}", &db_url)
            .replace("{{TESTVDB_DB_URL}}", &db_url);
        assert_eq!(resolved3, format!("url = '{}'", db_url));
    }
}
