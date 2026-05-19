use super::{SafetyNet, TargetPlugin, TargetStyle};
use crate::agent::oracle::{InvariantCheck, InvariantSource};
use crate::agent::probe_milvus as milvus_probe;
use crate::agent::probe_milvus_advanced as milvus_advanced;
use crate::contract::schema::{CheckType, StructuredContract};
use crate::review::milvus::MilvusIndependentReviewer;
use crate::review::IndependentReviewer;
use crate::sandbox::manager::SidecarSpec;

pub struct MilvusPlugin;

impl TargetPlugin for MilvusPlugin {
    fn name(&self) -> &str {
        "milvus"
    }

    fn target_image(&self, version: &str) -> String {
        if version.starts_with('v') {
            format!("milvusdb/milvus:{}", version)
        } else {
            format!("milvusdb/milvus:v{}", version)
        }
    }

    fn pip_packages(&self) -> Vec<String> {
        vec!["requests".to_string(), "httpx".to_string(), "pymilvus==2.6.0".to_string()]
    }

    fn db_port(&self) -> u16 {
        19530
    }

    fn safety_nets(&self) -> Vec<SafetyNet> {
        let mut nets = Vec::new();

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "search_limit_zero".into(), param: "limit".into(), value: "0".into(), label: "limit=0".into(), redundant_with_mutation: false }.to_search_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "search_limit_negative".into(), param: "limit".into(), value: "-1".into(), label: "limit=-1".into(), redundant_with_mutation: false }.to_search_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "search_offset_negative".into(), param: "offset".into(), value: "-1".into(), label: "offset=-1".into(), redundant_with_mutation: false }.to_search_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "search_nprobe_zero".into(), param: "nprobe".into(), value: "0".into(), label: "nprobe=0".into(), redundant_with_mutation: false }.to_search_params_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "search_nprobe_negative".into(), param: "nprobe".into(), value: "-1".into(), label: "nprobe=-1".into(), redundant_with_mutation: false }.to_search_params_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "dim_zero".into(), param: "dim".into(), value: "0".into(), label: "dim=0".into(), redundant_with_mutation: false }.to_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "dim_negative".into(), param: "dim".into(), value: "-1".into(), label: "dim=-1".into(), redundant_with_mutation: false }.to_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "dim_oversized".into(), param: "dim".into(), value: "999999".into(), label: "dim=999999".into(), redundant_with_mutation: false }.to_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "invalid_metric".into(), param: "metricType".into(), value: "INVALID".into(), label: "metricType=INVALID".into(), redundant_with_mutation: false }.to_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "invalid_index_type".into(), param: "indexType".into(), value: "InvalidIndex".into(), label: "indexType=InvalidIndex".into(), redundant_with_mutation: false }.to_create_safety_net());

        nets.push(SafetyNet { name: "nan_vector_search".into(), script: milvus_probe::milvus_nan_vector_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "inf_vector_search".into(), script: milvus_probe::milvus_inf_vector_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "empty_vector_search".into(), script: milvus_probe::milvus_empty_vector_search_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "nan_vector_insert".into(), script: milvus_probe::milvus_upsert_nan_vector_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "inf_vector_insert".into(), script: milvus_probe::milvus_upsert_inf_vector_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "duplicate_collection".into(), script: milvus_probe::milvus_duplicate_collection_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "invalid_distance".into(), script: milvus_probe::milvus_invalid_metric_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "invalid_index_type_check".into(), script: milvus_probe::milvus_invalid_index_type_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "search_nonexistent".into(), script: milvus_probe::milvus_search_nonexistent(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "wrong_dimension_insert".into(), script: milvus_probe::milvus_wrong_dimension_insert_check(), redundant_with_mutation: false });

        nets.push(SafetyNet { name: "insert_count_consistency".into(), script: milvus_probe::milvus_count_consistency_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "delete_count_consistency".into(), script: milvus_probe::milvus_delete_count_consistency_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "upsert_idempotency".into(), script: milvus_probe::milvus_upsert_idempotency_check(), redundant_with_mutation: true });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "index_invalid_type".into(), param: "indexType".into(), value: "InvalidIndex".into(), label: "invalid indexType".into(), redundant_with_mutation: false }.to_index_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "index_nlist_zero".into(), param: "nlist".into(), value: "0".into(), label: "nlist=0".into(), redundant_with_mutation: false }.to_index_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "index_M_zero".into(), param: "M".into(), value: "0".into(), label: "M=0".into(), redundant_with_mutation: false }.to_index_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "index_efConstruction_zero".into(), param: "efConstruction".into(), value: "0".into(), label: "efConstruction=0".into(), redundant_with_mutation: false }.to_index_create_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "index_metric_incompatible".into(), param: "metricType".into(), value: "HAMMING".into(), label: "metricType=HAMMING on FloatVector".into(), redundant_with_mutation: false }.to_index_create_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "hybrid_empty_search_params".into(), param: "searchParams".into(), value: "[]".into(), label: "empty searchParams".into(), redundant_with_mutation: false }.to_hybrid_search_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "hybrid_invalid_rerank".into(), param: "rerank".into(), value: r#"{"strategy":"invalid","params":{}}"#.into(), label: "invalid rerank strategy".into(), redundant_with_mutation: false }.to_hybrid_search_safety_net());

        nets.push(SafetyNet { name: "drop_nonexistent_index".into(), script: milvus_probe::milvus_drop_nonexistent_index(), redundant_with_mutation: false });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "describe_empty_collection".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for index describe".into(), redundant_with_mutation: false }.to_index_describe_safety_net());
        nets.push(SafetyNet { name: "describe_nonexistent_index".into(), script: milvus_probe::milvus_describe_nonexistent_index(), redundant_with_mutation: false });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "partition_empty_name".into(), param: "partitionName".into(), value: "''".into(), label: "empty partition name".into(), redundant_with_mutation: false }.to_partition_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "partition_special_chars".into(), param: "partitionName".into(), value: r#""test'; DROP--""#.into(), label: "SQL injection in partition name".into(), redundant_with_mutation: false }.to_partition_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "partition_drop_empty_name".into(), param: "partitionName".into(), value: "''".into(), label: "empty partition name for drop".into(), redundant_with_mutation: false }.to_partition_drop_safety_net());
        nets.push(SafetyNet { name: "drop_nonexistent_partition".into(), script: milvus_probe::milvus_drop_nonexistent_partition(), redundant_with_mutation: false });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "rename_empty_new_name".into(), param: "newCollectionName".into(), value: "''".into(), label: "empty new collection name".into(), redundant_with_mutation: false }.to_rename_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "rename_empty_old_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty old collection name for rename".into(), redundant_with_mutation: false }.to_rename_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alter_empty_properties".into(), param: "properties".into(), value: "{}".into(), label: "empty properties".into(), redundant_with_mutation: false }.to_alter_properties_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alter_invalid_ttl".into(), param: "properties".into(), value: r#"{"collection.ttl.seconds":-1}"#.into(), label: "negative TTL".into(), redundant_with_mutation: false }.to_alter_properties_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "add_field_empty_name".into(), param: "fieldName".into(), value: "''".into(), label: "empty field name".into(), redundant_with_mutation: false }.to_add_field_safety_net());
        nets.push(SafetyNet { name: "add_duplicate_vector_field".into(), script: milvus_probe::milvus_add_vector_field_check(), redundant_with_mutation: false });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "get_empty_ids".into(), param: "id".into(), value: "[]".into(), label: "empty id array".into(), redundant_with_mutation: false }.to_get_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "get_nonexistent_ids".into(), param: "id".into(), value: "[999999]".into(), label: "nonexistent entity IDs".into(), redundant_with_mutation: true }.to_get_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alias_create_empty".into(), param: "aliasName".into(), value: "''".into(), label: "empty alias name".into(), redundant_with_mutation: false }.to_alias_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alias_alter_nonexistent".into(), param: "collectionName".into(), value: "'nonexistent_col'".into(), label: "alias alter to nonexistent collection".into(), redundant_with_mutation: false }.to_alias_alter_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alias_drop_empty".into(), param: "aliasName".into(), value: "''".into(), label: "empty alias name for drop".into(), redundant_with_mutation: false }.to_alias_drop_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "db_create_duplicate".into(), param: "dbName".into(), value: "'default'".into(), label: "duplicate database name".into(), redundant_with_mutation: false }.to_database_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "db_drop_default".into(), param: "dbName".into(), value: "'default'".into(), label: "drop default database".into(), redundant_with_mutation: false }.to_database_drop_safety_net());
        nets.push(SafetyNet { name: "drop_nonexistent_database".into(), script: milvus_probe::milvus_drop_nonexistent_database(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "delete_empty_filter".into(), script: milvus_probe::milvus_delete_empty_filter_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "delete_null_filter".into(), script: milvus_probe::milvus_delete_null_filter_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "delete_nonexistent_id".into(), script: milvus_probe::milvus_delete_nonexistent_id_check(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "delete_then_query".into(), script: milvus_probe::milvus_delete_then_query_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "drop_nonexistent_collection".into(), script: milvus_probe::milvus_drop_nonexistent_collection(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "drop_then_describe".into(), script: milvus_probe::milvus_drop_then_describe_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "describe_nonexistent_collection".into(), script: milvus_probe::milvus_describe_nonexistent_collection(), redundant_with_mutation: false });

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "coll_list_empty_db".into(), param: "dbName".into(), value: "''".into(), label: "empty dbName for list".into(), redundant_with_mutation: false }.to_collection_list_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "coll_has_nonexistent".into(), param: "collectionName".into(), value: "'nonexistent_col'".into(), label: "has nonexistent collection".into(), redundant_with_mutation: true }.to_collection_has_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "coll_stats_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for stats".into(), redundant_with_mutation: false }.to_collection_stats_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "load_nonexistent".into(), param: "collectionName".into(), value: "'nonexistent_col'".into(), label: "load nonexistent collection".into(), redundant_with_mutation: false }.to_collection_mgmt_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "release_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for release".into(), redundant_with_mutation: false }.to_collection_release_safety_net());

        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "idx_list_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for index list".into(), redundant_with_mutation: false }.to_index_list_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "part_list_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for partition list".into(), redundant_with_mutation: false }.to_partition_list_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "part_has_nonexistent".into(), param: "partitionName".into(), value: "'nonexistent_part'".into(), label: "has nonexistent partition".into(), redundant_with_mutation: true }.to_partition_has_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "alias_list_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for alias list".into(), redundant_with_mutation: false }.to_alias_list_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "db_list_invalid_param".into(), param: "invalidParam".into(), value: "'test'".into(), label: "invalid param for db list".into(), redundant_with_mutation: false }.to_database_list_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "flush_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for flush".into(), redundant_with_mutation: false }.to_collection_flush_safety_net());
        nets.push(milvus_probe::MilvusSimpleSafetyNet { name: "compact_empty_name".into(), param: "collectionName".into(), value: "''".into(), label: "empty collectionName for compact".into(), redundant_with_mutation: false }.to_collection_compact_safety_net());

        nets.extend(milvus_probe::milvus_create_mutation_probes());
        nets.extend(milvus_probe::milvus_insert_mutation_probes());
        nets.extend(milvus_probe::milvus_search_mutation_probes());
        nets.extend(milvus_probe::milvus_query_mutation_probes());
        nets.extend(milvus_probe::milvus_upsert_mutation_probes());
        nets.extend(milvus_probe::milvus_index_create_mutation_probes());
        nets.extend(milvus_probe::milvus_delete_mutation_probes());
        nets.extend(milvus_probe::milvus_partition_create_mutation_probes());
        nets.extend(milvus_probe::milvus_database_create_mutation_probes());

        nets.push(SafetyNet { name: "l2_distance_ordering".into(), script: milvus_probe::milvus_l2_distance_ordering_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "ip_distance_ordering".into(), script: milvus_probe::milvus_ip_distance_ordering_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "hamming_search_check".into(), script: milvus_probe::milvus_hamming_search_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "jaccard_search_check".into(), script: milvus_probe::milvus_jaccard_search_check(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "auto_id_check".into(), script: milvus_probe::milvus_auto_id_check(), redundant_with_mutation: true });

        nets.push(SafetyNet { name: "metamorphic_nprobe".into(), script: milvus_advanced::metamorphic_nprobe_monotonicity(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "metamorphic_ef_search".into(), script: milvus_advanced::metamorphic_ef_search_monotonicity(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "metamorphic_query_consistency".into(), script: milvus_advanced::metamorphic_query_consistency(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "metamorphic_insert_monotonicity".into(), script: milvus_advanced::metamorphic_insert_monotonicity(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "metamorphic_limit".into(), script: milvus_advanced::metamorphic_limit_monotonicity(), redundant_with_mutation: true });

        nets.push(SafetyNet { name: "diff_create_collection".into(), script: milvus_advanced::diff_create_collection(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_insert".into(), script: milvus_advanced::diff_insert(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_search".into(), script: milvus_advanced::diff_search(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_query".into(), script: milvus_advanced::diff_query(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_delete".into(), script: milvus_advanced::diff_delete(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_create_index".into(), script: milvus_advanced::diff_create_index(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_describe".into(), script: milvus_advanced::diff_describe(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "diff_upsert".into(), script: milvus_advanced::diff_upsert(), redundant_with_mutation: true });

        for (i, seq_script) in milvus_advanced::generate_milvus_sequences().into_iter().enumerate() {
            nets.push(SafetyNet { name: format!("sequence_{}", i + 1), script: seq_script, redundant_with_mutation: true });
        }

        nets.push(SafetyNet { name: "concurrent_insert_search".into(), script: milvus_advanced::concurrent_insert_search(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "concurrent_delete_query".into(), script: milvus_advanced::concurrent_delete_query(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "concurrent_upsert_search".into(), script: milvus_advanced::concurrent_upsert_search(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "concurrent_create_drop".into(), script: milvus_advanced::concurrent_create_drop(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "concurrent_insert_flush".into(), script: milvus_advanced::concurrent_insert_flush(), redundant_with_mutation: true });

        nets.push(SafetyNet { name: "flat_l2_distance_ordering".into(), script: milvus_advanced::flat_index_l2_distance_ordering(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "flat_cosine_distance_ordering".into(), script: milvus_advanced::flat_index_cosine_distance_ordering(), redundant_with_mutation: true });

        nets.push(SafetyNet { name: "state_insert_search_delete_search".into(), script: milvus_advanced::state_insert_search_delete_search(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "state_insert_delete_insert_search".into(), script: milvus_advanced::state_insert_delete_insert_search(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "state_upsert_changes_vector".into(), script: milvus_advanced::state_upsert_changes_vector(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "state_create_drop_create_dim".into(), script: milvus_advanced::state_create_drop_create_different_dim(), redundant_with_mutation: true });
        nets.push(SafetyNet { name: "state_partition_data_isolation".into(), script: milvus_advanced::state_partition_create_drop_data_isolation(), redundant_with_mutation: true });

        nets.push(SafetyNet { name: "resource_large_dimension".into(), script: milvus_advanced::resource_large_dimension(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "resource_long_collection_name".into(), script: milvus_advanced::resource_long_collection_name(), redundant_with_mutation: false });
        nets.push(SafetyNet { name: "resource_zero_dimension".into(), script: milvus_advanced::resource_zero_dimension(), redundant_with_mutation: false });

        nets.extend(milvus_advanced::milvus_param_combination_probes());

        nets
    }

    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> {
        Some(Box::new(MilvusIndependentReviewer))
    }

    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();

        for assertion in &contract.assertions {
            let a_lower = assertion.to_lowercase();

            if a_lower.contains("limit") && a_lower.contains("> 0") {
                checks.push(InvariantCheck {
                    name: "search_limit_positive".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_search_probe("limit", "0", "limit=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("offset") && a_lower.contains(">=") {
                checks.push(InvariantCheck {
                    name: "search_offset_nonneg".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_search_probe("offset", "-1", "offset=-1"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("dim") && (a_lower.contains("> 0") || a_lower.contains("must not be 0")) {
                checks.push(InvariantCheck {
                    name: "create_dim_positive".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_create_probe("dim", "0", "dim=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("metrictype") && a_lower.contains("must be one of") {
                checks.push(InvariantCheck {
                    name: "create_valid_metric".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_create_probe("metricType", "INVALID", "metricType=INVALID"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("indextype") && a_lower.contains("valid") {
                checks.push(InvariantCheck {
                    name: "create_valid_index".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_create_probe("indexType", "InvalidIndex", "indexType=InvalidIndex"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("nprobe") && a_lower.contains("> 0") {
                checks.push(InvariantCheck {
                    name: "search_nprobe_positive".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_search_params_probe("nprobe", "0", "nprobe=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("nan") || a_lower.contains("infinity") {
                checks.push(InvariantCheck {
                    name: "search_no_nan_inf".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_nan_vector_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("non-existent") || a_lower.contains("nonexistent") {
                checks.push(InvariantCheck {
                    name: "search_nonexistent_collection".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_search_nonexistent(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("duplicate") && a_lower.contains("collection") {
                checks.push(InvariantCheck {
                    name: "create_no_duplicate".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_duplicate_collection_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("rowcount") && a_lower.contains("must equal") {
                checks.push(InvariantCheck {
                    name: "insert_count_consistency".into(),
                    check_type: CheckType::CountConsistency,
                    script: milvus_probe::milvus_count_consistency_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("nlist") && a_lower.contains("> 0") {
                checks.push(InvariantCheck {
                    name: "index_nlist_positive".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_index_probe("nlist", "0", "nlist=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("efconstruction") && a_lower.contains("> 0") {
                checks.push(InvariantCheck {
                    name: "index_efconstruction_positive".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_index_probe("efConstruction", "0", "efConstruction=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("rerank") && (a_lower.contains("must be") || a_lower.contains("valid")) {
                checks.push(InvariantCheck {
                    name: "hybrid_valid_rerank".into(),
                    check_type: CheckType::ValueRange,
                    script: milvus_probe::milvus_hybrid_search_probe("rerank", r#"{"strategy":"invalid","params":{}}"#, "invalid rerank strategy"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("partitionname") && a_lower.contains("non-empty") {
                checks.push(InvariantCheck {
                    name: "partition_name_required".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_partition_probe("partitionName", "''", "empty partition name"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("dropping nonexistent") && a_lower.contains("partition") {
                checks.push(InvariantCheck {
                    name: "partition_drop_nonexistent".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_drop_nonexistent_partition(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("aliasname") && a_lower.contains("non-empty") {
                checks.push(InvariantCheck {
                    name: "alias_name_required".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_alias_probe("aliasName", "''", "empty alias name"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("dropping nonexistent") && a_lower.contains("index") {
                checks.push(InvariantCheck {
                    name: "index_drop_nonexistent".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_drop_nonexistent_index(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("dropping default database") {
                checks.push(InvariantCheck {
                    name: "db_drop_default_rejected".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_database_drop_probe("dbName", "'default'", "drop default database"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("loading nonexistent") {
                checks.push(InvariantCheck {
                    name: "load_nonexistent_collection".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_collection_mgmt_probe("collectionName", "'nonexistent_col'", "load nonexistent collection"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("releasing unloaded") {
                checks.push(InvariantCheck {
                    name: "release_unloaded_collection".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_collection_release_probe("collectionName", "'nonexistent_col'", "release unloaded collection"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("adding vector field") && a_lower.contains("already has") {
                checks.push(InvariantCheck {
                    name: "add_duplicate_vector_field".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_add_vector_field_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("collectionname") && a_lower.contains("non-empty") && a_lower.contains("flush") {
                checks.push(InvariantCheck {
                    name: "flush_collection_required".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_collection_flush_probe("collectionName", "''", "empty collectionName for flush"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("collectionname") && a_lower.contains("non-empty") && a_lower.contains("compact") {
                checks.push(InvariantCheck {
                    name: "compact_collection_required".into(),
                    check_type: CheckType::ExistenceCheck,
                    script: milvus_probe::milvus_collection_compact_probe("collectionName", "''", "empty collectionName for compact"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }
        }

        let mut seen = std::collections::HashSet::new();
        checks.retain(|c| seen.insert(c.name.clone()));

        checks
    }

    fn db_sidecars(&self) -> Vec<SidecarSpec> {
        vec![
            SidecarSpec {
                image: "quay.io/coreos/etcd:v3.5.18".to_string(),
                hostname: "etcd".to_string(),
                env: vec![],
                command: vec![
                    "etcd".to_string(),
                    "-advertise-client-urls".to_string(),
                    "http://etcd:2379".to_string(),
                    "-listen-client-urls".to_string(),
                    "http://0.0.0.0:2379".to_string(),
                ],
            },
            SidecarSpec {
                image: "minio/minio:RELEASE.2024-12-18T13-15-44Z".to_string(),
                hostname: "milvus-minio".to_string(),
                env: vec![
                    ("MINIO_ACCESS_KEY".to_string(), "minioadmin".to_string()),
                    ("MINIO_SECRET_KEY".to_string(), "minioadmin".to_string()),
                ],
                command: vec![
                    "minio".to_string(),
                    "server".to_string(),
                    "/minio_data".to_string(),
                ],
            },
        ]
    }

    fn db_env(&self) -> Vec<(String, String)> {
        vec![
            ("ETCD_ENDPOINTS".to_string(), "etcd:2379".to_string()),
            ("MINIO_ADDRESS".to_string(), "milvus-minio:9000".to_string()),
        ]
    }

    fn db_command(&self) -> Vec<String> {
        vec!["milvus".to_string(), "run".to_string(), "standalone".to_string()]
    }

    fn target_style(&self) -> TargetStyle {
        TargetStyle::Milvus
    }

    fn doc_citation_url(&self) -> String {
        "https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milvus_plugin_metadata() {
        let plugin = MilvusPlugin;
        assert_eq!(plugin.name(), "milvus");
        assert_eq!(plugin.target_image("2.6.16"), "milvusdb/milvus:v2.6.16");
        assert_eq!(plugin.target_image("v2.6.16"), "milvusdb/milvus:v2.6.16");
        assert_eq!(plugin.db_port(), 19530);
    }

    #[test]
    fn test_milvus_pip_packages() {
        let plugin = MilvusPlugin;
        let pkgs = plugin.pip_packages();
        assert!(pkgs.contains(&"requests".to_string()));
    }

    #[test]
    fn test_milvus_safety_nets_count() {
        let plugin = MilvusPlugin;
        let nets = plugin.safety_nets();
        assert_eq!(nets.len(), 205);
    }

    #[test]
    fn test_milvus_derive_oracle_checks() {
        let plugin = MilvusPlugin;
        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "https://milvus.io/docs/".to_string(),
            assertions: vec!["limit must be > 0".to_string()],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };
        let checks = plugin.derive_oracle_checks(&contract);
        assert!(!checks.is_empty());
    }
}
