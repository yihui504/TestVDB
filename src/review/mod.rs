use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::agent::classifier::DefectType;
use crate::sandbox::manager::Sandbox;

pub mod milvus;
pub mod qdrant;

pub type ReviewResult = Value;

#[async_trait]
pub trait IndependentReviewer: Send + Sync {
    fn target_name(&self) -> &str;

    async fn run_probe(
        &self,
        sandbox: &Sandbox,
        port: u16,
    ) -> Result<ReviewResult>;

    fn summarize_findings(
        &self,
        probe_json: &ReviewResult,
    ) -> Option<(DefectType, Vec<String>)>;
}
