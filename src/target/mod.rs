use crate::agent::oracle::InvariantCheck;
use crate::agent::probe::ProbeTemplate;
use crate::contract::schema::StructuredContract;
use crate::review::IndependentReviewer;
use crate::sandbox::manager::SidecarSpec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod milvus;
pub mod qdrant;
pub mod weaviate;
pub mod pgvector;

pub use milvus::MilvusPlugin;
pub use qdrant::QdrantPlugin;
pub use weaviate::WeaviatePlugin;
pub use pgvector::PgVectorPlugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStyle {
    Qdrant,
    Milvus,
    Weaviate,
    PgVector,
}

#[derive(Clone)]
pub struct SafetyNet {
    pub name: String,
    pub script: String,
    pub redundant_with_mutation: bool,
}

impl Default for SafetyNet {
    fn default() -> Self {
        SafetyNet {
            name: String::new(),
            script: String::new(),
            redundant_with_mutation: false,
        }
    }
}

pub trait TargetPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn target_image(&self, version: &str) -> String;
    fn pip_packages(&self) -> Vec<String>;
    fn db_port(&self) -> u16;
    fn safety_nets(&self) -> Vec<SafetyNet>;
    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>>;
    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck>;
    fn target_style(&self) -> TargetStyle;
    fn doc_citation_url(&self) -> String;
    fn probe_template(&self) -> &dyn ProbeTemplate;
    fn db_sidecars(&self) -> Vec<SidecarSpec> { Vec::new() }
    fn db_env(&self) -> Vec<(String, String)> { Vec::new() }
    fn db_command(&self) -> Vec<String> { Vec::new() }
    /// Default source repository URL for Knowledge Agent auto-trigger.
    fn default_repo_url(&self) -> Option<&str> { None }
    /// Default documentation URL for Knowledge Agent auto-trigger.
    fn default_docs_url(&self) -> Option<&str> { None }
}

pub struct TargetRegistry {
    plugins: HashMap<String, Box<dyn TargetPlugin>>,
}

impl TargetRegistry {
    pub fn new() -> Self {
        TargetRegistry {
            plugins: HashMap::new(),
        }
    }

    /// Pre-registers all four supported vector databases.
    pub fn new_with_all() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(MilvusPlugin));
        registry.register(Box::new(QdrantPlugin));
        registry.register(Box::new(WeaviatePlugin));
        registry.register(Box::new(PgVectorPlugin));
        registry
    }

    pub fn register(&mut self, plugin: Box<dyn TargetPlugin>) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, plugin);
    }

    pub fn get(&self, target: &str) -> Option<&dyn TargetPlugin> {
        self.plugins.get(target).map(|p| p.as_ref())
    }

    pub fn available_targets(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }
}