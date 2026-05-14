use crate::agent::oracle::InvariantCheck;
use crate::contract::schema::StructuredContract;
use crate::review::IndependentReviewer;
use std::collections::HashMap;

pub mod qdrant;

pub use qdrant::QdrantPlugin;

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

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyPlugin;
    impl TargetPlugin for DummyPlugin {
        fn name(&self) -> &str { "dummy" }
        fn target_image(&self, version: &str) -> String { format!("dummy:v{}", version) }
        fn pip_packages(&self) -> Vec<String> { vec!["requests".to_string()] }
        fn db_port(&self) -> u16 { 8080 }
        fn safety_nets(&self) -> Vec<SafetyNet> { vec![] }
        fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> { None }
        fn derive_oracle_checks(&self, _contract: &StructuredContract) -> Vec<InvariantCheck> { vec![] }
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = TargetRegistry::new();
        registry.register(Box::new(DummyPlugin));
        assert!(registry.get("dummy").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_available_targets() {
        let mut registry = TargetRegistry::new();
        registry.register(Box::new(DummyPlugin));
        let targets = registry.available_targets();
        assert!(targets.contains(&"dummy"));
    }

    #[test]
    fn test_plugin_metadata() {
        let plugin = DummyPlugin;
        assert_eq!(plugin.name(), "dummy");
        assert_eq!(plugin.target_image("1.0"), "dummy:v1.0");
        assert_eq!(plugin.db_port(), 8080);
    }
}
