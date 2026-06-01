use anyhow::{bail, Context};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};
use uuid::Uuid;

const DB_READY_TIMEOUT_SECS: u64 = 60;
const DB_PROBE_INTERVAL_MS: u64 = 500;
const SIDECAR_WAIT_SECS: u64 = 5;

#[derive(Debug, Clone)]
pub struct SidecarSpec {
    pub image: String,
    pub hostname: String,
    pub env: Vec<(String, String)>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct Sandbox {
    pub network_name: String,
    pub db_container_id: Option<String>,
    pub runner_container_ids: Vec<String>,
    pub db_host: Option<String>,
    pub sidecar_container_ids: Vec<String>,
    db_image: String,
    db_port: u16,
    pip_packages: Vec<String>,
    sidecar_specs: Vec<SidecarSpec>,
    db_env: Vec<(String, String)>,
    db_command: Vec<String>,
}

struct CleanupGuard {
    network_name: String,
    db_container_id: Option<String>,
    runner_container_ids: Vec<String>,
    sidecar_container_ids: Vec<String>,
    disarmed: bool,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        for id in &self.runner_container_ids {
            let _ = std::process::Command::new("docker").args(["rm", "-f", id]).output();
        }
        if let Some(ref db) = self.db_container_id {
            let _ = std::process::Command::new("docker").args(["rm", "-f", db]).output();
        }
        for id in &self.sidecar_container_ids {
            let _ = std::process::Command::new("docker").args(["rm", "-f", id]).output();
        }
        let _ = std::process::Command::new("docker").args(["network", "rm", &self.network_name]).output();
    }
}

impl Sandbox {
    /// Creates an isolated Docker network, launches a target DB container, and a Python runner container.
    pub async fn create_network_and_containers(db_image: &str, pip_packages: &[&str], db_port: u16, sidecars: &[SidecarSpec], db_env: &[(String, String)], db_command: &[String]) -> anyhow::Result<Self> {
        let network_name = format!("testvdb-net-{}", Uuid::new_v4().simple());
        let db_name = format!("testvdb-db-{}", Uuid::new_v4().simple());
        let runner_name = format!("testvdb-runner-{}", Uuid::new_v4().simple());
        let db_image = db_image.to_string();
        let pip_packages_owned: Vec<String> = pip_packages.iter().map(|s| s.to_string()).collect();
        let sidecar_specs = sidecars.to_vec();
        let db_env = db_env.to_vec();
        let db_command = db_command.to_vec();

        info!("Creating Docker network: {}", network_name);
        let out = Command::new("docker").args(["network", "create", &network_name]).output().await?;
        if !out.status.success() { bail!("Failed to create network: {}", String::from_utf8_lossy(&out.stderr)); }

        let mut guard = CleanupGuard {
            network_name: network_name.clone(),
            db_container_id: None,
            runner_container_ids: Vec::new(),
            sidecar_container_ids: Vec::new(),
            disarmed: false,
        };

        let mut sidecar_container_ids = Vec::new();
        for spec in sidecars {
            let sidecar_name = format!("testvdb-sidecar-{}", Uuid::new_v4().simple());
            info!("Starting sidecar container: {} as {}", spec.image, sidecar_name);
            let mut sidecar_args: Vec<String> = vec![
                "run".to_string(), "-d".to_string(),
                "--name".to_string(), sidecar_name,
                "--network".to_string(), network_name.clone(),
                "--hostname".to_string(), spec.hostname.clone(),
            ];
            for (key, value) in &spec.env {
                sidecar_args.push("-e".to_string());
                sidecar_args.push(format!("{}={}", key, value));
            }
            sidecar_args.push(spec.image.clone());
            sidecar_args.extend(spec.command.iter().cloned());
            let out = Command::new("docker").args(&sidecar_args).output().await?;
            if !out.status.success() {
                bail!("Failed to start sidecar {}: {}", spec.hostname, String::from_utf8_lossy(&out.stderr));
            }
            let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
            sidecar_container_ids.push(container_id.clone());
            guard.sidecar_container_ids.push(container_id);
        }

        if !sidecar_container_ids.is_empty() {
            info!("Waiting 5s for sidecar containers to initialize...");
            tokio::time::sleep(tokio::time::Duration::from_secs(SIDECAR_WAIT_SECS)).await;
        }

        info!("Starting DB container: {} as {}", db_image, db_name);
        let mut db_args: Vec<String> = vec!["run".to_string(), "-d".to_string(), "--name".to_string(), db_name.clone(), "--network".to_string(), network_name.clone()];
        for (key, value) in &db_env {
            db_args.push("-e".to_string());
            db_args.push(format!("{}={}", key, value));
        }
        db_args.push(db_image.to_string());
        db_args.extend(db_command.iter().cloned());
        let out = Command::new("docker").args(&db_args).output().await?;
        if !out.status.success() { 
            bail!("Failed to start DB: {}", String::from_utf8_lossy(&out.stderr)); 
        }
        let db_container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        guard.db_container_id = Some(db_container_id.clone());

        info!("Starting Runner container: python:3.9-slim as {}", runner_name);
        let out = Command::new("docker")
            .args(["run", "-d", "--name", &runner_name, "--network", &network_name, "python:3.9-slim", "tail", "-f", "/dev/null"])
            .output().await?;
        if !out.status.success() { 
            bail!("Failed to start Runner: {}", String::from_utf8_lossy(&out.stderr)); 
        }
        let runner_container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        guard.runner_container_ids.push(runner_container_id.clone());

        if !pip_packages.is_empty() {
            info!("Installing pip packages in Runner...");
            let pip_cmd = crate::infra::pip_install_args(&runner_container_id, pip_packages);
            let out = Command::new("docker").args(&pip_cmd).output().await?;
            if !out.status.success() { 
                bail!("Failed to install pip packages: {}", String::from_utf8_lossy(&out.stderr)); 
            }
        }

        // Wait for DB to be ready by polling TCP connectivity from the runner container
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(DB_READY_TIMEOUT_SECS);
        loop {
            let probe_script = format!(
                "import socket; s=socket.socket(); s.settimeout(0.5); s.connect(('{}', {})); s.close()",
                db_name, db_port
            );
            let out = Command::new("docker")
                .args(["exec", &runner_container_id, "python", "-c", &probe_script])
                .output().await?;
            if out.status.success() {
                info!("DB container is ready on {}:{}", db_name, db_port);
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "Timed out waiting for DB container to become ready on {}:{}. Last probe stderr: {}",
                    db_name,
                    db_port,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(DB_PROBE_INTERVAL_MS)).await;
        }

        // Success! Disarm the guard
        guard.disarmed = true;

        Ok(Self {
            network_name,
            db_container_id: Some(db_container_id),
            runner_container_ids: vec![runner_container_id],
            db_host: Some(db_name),
            sidecar_container_ids,
            db_image,
            db_port,
            pip_packages: pip_packages_owned,
            sidecar_specs,
            db_env,
            db_command,
        })
    }

    pub async fn create_knowledge_worker(image: &str, apt_packages: &[&str]) -> anyhow::Result<Self> {
        let network_name = format!("testvdb-net-{}", Uuid::new_v4().simple());
        let runner_name = format!("testvdb-worker-{}", Uuid::new_v4().simple());

        info!("Creating Docker network: {}", network_name);
        let out = Command::new("docker").args(["network", "create", &network_name]).output().await?;
        if !out.status.success() { bail!("Failed to create network: {}", String::from_utf8_lossy(&out.stderr)); }

        let mut guard = CleanupGuard {
            network_name: network_name.clone(),
            db_container_id: None,
            runner_container_ids: Vec::new(),
            sidecar_container_ids: Vec::new(),
            disarmed: false,
        };

        info!("Starting Knowledge Worker container: {} as {}", image, runner_name);
        // Using ubuntu instead of alpine because apt-get is used for package management.
        let out = Command::new("docker")
            .args(["run", "-d", "--name", &runner_name, "--network", &network_name, "ubuntu:latest", "tail", "-f", "/dev/null"])
            .output().await?;
        if !out.status.success() { 
            bail!("Failed to start Knowledge Worker: {}", String::from_utf8_lossy(&out.stderr)); 
        }
        let runner_container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        guard.runner_container_ids.push(runner_container_id.clone());

        if !apt_packages.is_empty() {
            info!("Installing apt packages in Knowledge Worker...");
            // Run apt-get update
            let update_out = Command::new("docker").args(["exec", &runner_container_id, "apt-get", "update"]).output().await?;
            if !update_out.status.success() { 
                bail!("Failed to update apt in Knowledge Worker: {}", String::from_utf8_lossy(&update_out.stderr)); 
            }
            
            let mut apt_cmd = vec!["exec", &runner_container_id, "apt-get", "install", "-y", "--no-install-recommends"];
            apt_cmd.extend(apt_packages);
            let out = Command::new("docker").args(&apt_cmd).output().await?;
            if !out.status.success() { 
                bail!("Failed to install apt packages in Knowledge Worker: {}", String::from_utf8_lossy(&out.stderr)); 
            }
        }

        guard.disarmed = true;

        Ok(Self {
            network_name,
            db_container_id: None,
            runner_container_ids: vec![runner_container_id],
            db_host: None,
            sidecar_container_ids: Vec::new(),
            db_image: String::new(),
            db_port: 0,
            pip_packages: Vec::new(),
            sidecar_specs: Vec::new(),
            db_env: Vec::new(),
            db_command: Vec::new(),
        })
    }

    pub async fn create_shared_runner(&self, pip_packages: &[&str]) -> anyhow::Result<String> {
        let runner_name = format!("testvdb-runner-{}", Uuid::new_v4().simple());
        let out = Command::new("docker")
            .args(["run", "-d", "--name", &runner_name, "--network", &self.network_name, "python:3.9-slim", "tail", "-f", "/dev/null"])
            .output().await?;
        if !out.status.success() {
            bail!("Failed to start shared Runner: {}", String::from_utf8_lossy(&out.stderr));
        }
        let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !pip_packages.is_empty() {
            let pip_cmd = crate::infra::pip_install_args(&container_id, pip_packages);
            let out = Command::new("docker").args(&pip_cmd).output().await?;
            if !out.status.success() {
                bail!("Failed to install pip packages in shared Runner: {}", String::from_utf8_lossy(&out.stderr));
            }
        }
        Ok(container_id)
    }

    /// Executes a command inside the isolated Runner container and returns full process output.
    pub async fn exec_command(&self, cmd: &[&str]) -> anyhow::Result<ExecutionOutput> {
        self.exec_command_with_env(cmd, &[]).await
    }

    /// Executes a command with additional environment variables.
    pub async fn exec_command_with_env(&self, cmd: &[&str], env_vars: &[(&str, &str)]) -> anyhow::Result<ExecutionOutput> {
        let mut docker_args: Vec<String> = vec!["exec".to_string()];
        for (key, value) in env_vars {
            docker_args.push("-e".to_string());
            docker_args.push(format!("{}={}", key, value));
        }
        docker_args.push(self.runner_container_ids.first().cloned().unwrap_or_default());
        docker_args.extend(cmd.iter().map(|s| s.to_string()));

        let output = Command::new("docker")
            .args(&docker_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute docker exec command")?;

        Ok(ExecutionOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }

    pub async fn exec_script(&self, script: &str, env_vars: &[(&str, &str)]) -> anyhow::Result<ExecutionOutput> {
        let script_path = "/tmp/testvdb_script.py";
        let write_args: Vec<String> = vec![
            "exec".to_string(),
            "-i".to_string(),
            self.runner_container_ids.first().unwrap_or(&String::new()).clone(),
            "bash".to_string(),
            "-c".to_string(),
            format!("cat > {} << 'TESTVDB_SCRIPT_EOF'\n{}\nTESTVDB_SCRIPT_EOF", script_path, script),
        ];
        let write_output = Command::new("docker")
            .args(&write_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to write script to container")?;
        if !write_output.status.success() {
            anyhow::bail!("Failed to write script to container: {}", String::from_utf8_lossy(&write_output.stderr));
        }

        self.exec_command_with_env(&["python", "-u", script_path], env_vars).await
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for id in &self.runner_container_ids {
            let _ = std::process::Command::new("docker").args(["rm", "-f", id]).output();
        }
        if let Some(ref db_id) = self.db_container_id {
            let _ = std::process::Command::new("docker").args(["rm", "-f", db_id]).output();
        }
        for id in &self.sidecar_container_ids {
            let _ = std::process::Command::new("docker").args(["rm", "-f", id]).output();
        }
        let _ = std::process::Command::new("docker").args(["network", "rm", &self.network_name]).output();
    }
}

impl Sandbox {
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        for id in &self.runner_container_ids {
            let _ = Command::new("docker").args(["stop", id]).output().await;
            let _ = Command::new("docker").args(["rm", "-f", id]).output().await;
        }
        if let Some(ref db_id) = self.db_container_id {
            let _ = Command::new("docker").args(["stop", db_id]).output().await;
            let _ = Command::new("docker").args(["rm", "-f", db_id]).output().await;
        }
        for id in &self.sidecar_container_ids {
            let _ = Command::new("docker").args(["stop", id]).output().await;
            let _ = Command::new("docker").args(["rm", "-f", id]).output().await;
        }
        let _ = Command::new("docker").args(["network", "rm", &self.network_name]).output().await;
        Ok(())
    }

    pub async fn health_check(&self) -> bool {
        let container_ids: Vec<&str> = self.runner_container_ids.iter()
            .chain(self.db_container_id.iter())
            .chain(self.sidecar_container_ids.iter())
            .map(|s| s.as_str())
            .collect();

        for id in &container_ids {
            let out = Command::new("docker")
                .args(["inspect", "-f", "{{.State.Running}}", id])
                .output()
                .await;
            match out {
                Ok(o) if o.status.success() => {
                    let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if status != "true" {
                        warn!("Sandbox container {} is not running (status: {})", id, status);
                        return false;
                    }
                }
                _ => {
                    warn!("Sandbox container {} not found or unreachable", id);
                    return false;
                }
            }
        }
        true
    }

    pub async fn pollution_check(&self) -> anyhow::Result<bool> {
        for runner_id in &self.runner_container_ids {
            let df_out = Command::new("docker")
                .args(["exec", runner_id, "df", "/"])
                .output()
                .await?;
            if df_out.status.success() {
                let df_str = String::from_utf8_lossy(&df_out.stdout);
                for line in df_str.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let use_pct = parts[4].trim_end_matches('%');
                        if let Ok(pct) = use_pct.parse::<u32>() {
                            if pct > 95 {
                                warn!("Sandbox runner {} disk usage at {}%, considered polluted", runner_id, pct);
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }

        for runner_id in &self.runner_container_ids {
            let ps_out = Command::new("docker")
                .args(["exec", runner_id, "ps", "-eo", "stat,comm"])
                .output()
                .await?;
            if ps_out.status.success() {
                let ps_str = String::from_utf8_lossy(&ps_out.stdout);
                let zombie_count = ps_str.lines().filter(|l| l.starts_with('Z')).count();
                if zombie_count > 10 {
                    warn!("Sandbox runner {} has {} zombie processes, considered polluted", runner_id, zombie_count);
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub async fn ensure_healthy(&mut self) -> anyhow::Result<bool> {
        if !self.health_check().await {
            warn!("Sandbox health check failed, attempting restore...");
            self.force_cleanup().await;
            let restored = Sandbox::create_network_and_containers(
                &self.db_image,
                &self.pip_packages.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                self.db_port,
                &self.sidecar_specs,
                &self.db_env,
                &self.db_command,
            ).await?;
            *self = restored;
            info!("Sandbox restored successfully");
            return Ok(true);
        }

        if self.pollution_check().await? {
            warn!("Sandbox pollution detected, recreating...");
            self.force_cleanup().await;
            let restored = Sandbox::create_network_and_containers(
                &self.db_image,
                &self.pip_packages.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                self.db_port,
                &self.sidecar_specs,
                &self.db_env,
                &self.db_command,
            ).await?;
            *self = restored;
            info!("Sandbox recreated after pollution detection");
            return Ok(true);
        }

        Ok(false)
    }

    async fn force_cleanup(&self) {
        for id in &self.runner_container_ids {
            let _ = Command::new("docker").args(["rm", "-f", id]).output().await;
        }
        if let Some(ref db_id) = self.db_container_id {
            let _ = Command::new("docker").args(["rm", "-f", db_id]).output().await;
        }
        for id in &self.sidecar_container_ids {
            let _ = Command::new("docker").args(["rm", "-f", id]).output().await;
        }
        let _ = Command::new("docker").args(["network", "rm", &self.network_name]).output().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_lifecycle() {
        // Use a lightweight image for the "DB" to speed up testing
        let sandbox = Sandbox::create_network_and_containers("nginx:alpine", &[], 80, &[], &[], &[])
            .await
            .expect("Failed to create sandbox network and containers");

        assert!(!sandbox.network_name.is_empty());
        assert!(sandbox.db_container_id.is_some());
        assert!(!sandbox.runner_container_ids.is_empty());

        // Execute a command in the runner
        let output = sandbox
            .exec_command(&["echo", "hello_sandbox"])
            .await
            .expect("Failed to execute command in sandbox");

        assert!(output.success);
        assert_eq!(output.stdout.trim(), "hello_sandbox");

        // Cleanup
        sandbox.cleanup().await.expect("Failed to cleanup sandbox");
    }
}
