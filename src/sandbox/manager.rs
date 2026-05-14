use anyhow::{bail, Context};
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;
use uuid::Uuid;

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
    pub runner_container_id: String,
    pub db_host: Option<String>,
}

struct CleanupGuard {
    network_name: String,
    db_container_id: Option<String>,
    runner_container_id: Option<String>,
    disarmed: bool,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Some(ref runner) = self.runner_container_id {
            let _ = std::process::Command::new("docker").args(["rm", "-f", runner]).output();
        }
        if let Some(ref db) = self.db_container_id {
            let _ = std::process::Command::new("docker").args(["rm", "-f", db]).output();
        }
        let _ = std::process::Command::new("docker").args(["network", "rm", &self.network_name]).output();
    }
}

impl Sandbox {
    /// Creates an isolated Docker network, launches a target DB container, and a Python runner container.
    pub async fn create_network_and_containers(db_image: &str, pip_packages: &[&str], db_port: u16) -> anyhow::Result<Self> {
        let network_name = format!("testvdb-net-{}", Uuid::new_v4().simple());
        let db_name = format!("testvdb-db-{}", Uuid::new_v4().simple());
        let runner_name = format!("testvdb-runner-{}", Uuid::new_v4().simple());

        info!("Creating Docker network: {}", network_name);
        let out = Command::new("docker").args(["network", "create", &network_name]).output().await?;
        if !out.status.success() { bail!("Failed to create network: {}", String::from_utf8_lossy(&out.stderr)); }

        // RAII Drop Guard to automatically rollback resources if anything fails mid-way
        let mut guard = CleanupGuard {
            network_name: network_name.clone(),
            db_container_id: None,
            runner_container_id: None,
            disarmed: false,
        };

        info!("Starting DB container: {} as {}", db_image, db_name);
        let out = Command::new("docker")
            .args(["run", "-d", "--name", &db_name, "--network", &network_name, db_image])
            .output().await?;
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
        guard.runner_container_id = Some(runner_container_id.clone());

        if !pip_packages.is_empty() {
            info!("Installing pip packages in Runner...");
            let mut pip_cmd = vec!["exec", &runner_container_id, "pip", "install", "--no-cache-dir"];
            pip_cmd.extend(pip_packages);
            let out = Command::new("docker").args(&pip_cmd).output().await?;
            if !out.status.success() { 
                bail!("Failed to install pip packages: {}", String::from_utf8_lossy(&out.stderr)); 
            }
        }

        // Wait for DB to be ready by polling TCP connectivity from the runner container
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(60);
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
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Success! Disarm the guard
        guard.disarmed = true;

        Ok(Self {
            network_name,
            db_container_id: Some(db_container_id),
            runner_container_id,
            db_host: Some(db_name),
        })
    }

    /// Creates an isolated Docker network and launches a knowledge worker container
    /// (e.g., with git, curl) without a target database.
    pub async fn create_knowledge_worker(image: &str, apt_packages: &[&str]) -> anyhow::Result<Self> {
        let network_name = format!("testvdb-net-{}", Uuid::new_v4().simple());
        let runner_name = format!("testvdb-worker-{}", Uuid::new_v4().simple());

        info!("Creating Docker network: {}", network_name);
        let out = Command::new("docker").args(["network", "create", &network_name]).output().await?;
        if !out.status.success() { bail!("Failed to create network: {}", String::from_utf8_lossy(&out.stderr)); }

        let mut guard = CleanupGuard {
            network_name: network_name.clone(),
            db_container_id: None,
            runner_container_id: None,
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
        guard.runner_container_id = Some(runner_container_id.clone());

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
            runner_container_id,
            db_host: None,
        })
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
        docker_args.push(self.runner_container_id.clone());
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
            self.runner_container_id.clone(),
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

        self.exec_command_with_env(&["python", script_path], env_vars).await
    }

    /// Stops and removes the containers and network
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        let _ = Command::new("docker").args(["rm", "-f", &self.runner_container_id]).output().await;
        if let Some(ref db_id) = self.db_container_id {
            let _ = Command::new("docker").args(["rm", "-f", db_id]).output().await;
        }
        let _ = Command::new("docker").args(["network", "rm", &self.network_name]).output().await;
        Ok(())
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Fallback cleanup in case of panic
        let _ = std::process::Command::new("docker").args(["rm", "-f", &self.runner_container_id]).output();
        if let Some(ref db_id) = self.db_container_id {
            let _ = std::process::Command::new("docker").args(["rm", "-f", db_id]).output();
        }
        let _ = std::process::Command::new("docker").args(["network", "rm", &self.network_name]).output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_lifecycle() {
        // Use a lightweight image for the "DB" to speed up testing
        let sandbox = Sandbox::create_network_and_containers("nginx:alpine", &[], 80)
            .await
            .expect("Failed to create sandbox network and containers");

        assert!(!sandbox.network_name.is_empty());
        assert!(sandbox.db_container_id.is_some());
        assert!(!sandbox.runner_container_id.is_empty());

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
