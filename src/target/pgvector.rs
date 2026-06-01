use super::{SafetyNet, TargetPlugin, TargetStyle};
use crate::agent::oracle::{InvariantCheck, InvariantSource};
use crate::agent::probe::{ProbeTemplate, NoopProbeTemplate};
use crate::contract::schema::{BehaviorCategory, CheckType, StructuredContract};
use crate::review::IndependentReviewer;
use crate::review::pgvector::PgVectorIndependentReviewer;
use std::collections::HashSet;

pub fn pg_connect_code(host_expr: &str) -> String {
    format!(r#"psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={{{}}} port=5432")"#, host_expr)
}

pub fn pg_connect_uri(host: &str) -> String {
    format!("psycopg2.connect('postgresql://postgres:postgres@{}:5432/testvdb')", host)
}

pub struct PgVectorPlugin;

impl TargetPlugin for PgVectorPlugin {
    fn name(&self) -> &str {
        "pgvector"
    }

    fn target_image(&self, version: &str) -> String {
        if version.is_empty() {
            "pgvector/pgvector:pg17".to_string()
        } else {
            format!("pgvector/pgvector:{}", version)
        }
    }

    fn pip_packages(&self) -> Vec<String> {
        vec!["psycopg2-binary".to_string(), "uuid".to_string()]
    }

    fn db_port(&self) -> u16 {
        5432
    }

    fn default_repo_url(&self) -> Option<&str> {
        Some("https://github.com/pgvector/pgvector")
    }

    fn default_docs_url(&self) -> Option<&str> {
        Some("https://github.com/pgvector/pgvector")
    }

    fn safety_nets(&self) -> Vec<SafetyNet> {
        let mut nets = Vec::new();

        nets.push(SafetyNet {
            name: "dim_zero".into(),
            script: pgv_dim_zero_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_negative".into(),
            script: pgv_dim_negative_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_oversized".into(),
            script: pgv_dim_oversized_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "invalid_distance".into(),
            script: pgv_invalid_distance_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "hnsw_m_low".into(),
            script: pgv_hnsw_m_low_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "hnsw_ef_construction_low".into(),
            script: pgv_hnsw_ef_construction_low_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "ivfflat_lists_zero".into(),
            script: pgv_ivfflat_lists_zero_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_mismatch_insert".into(),
            script: pgv_dim_mismatch_probe(),
            redundant_with_mutation: false,
        });

        nets.push(SafetyNet {
            name: "state_insert_count".into(),
            script: pgv_state_count_probe(),
            redundant_with_mutation: true,
        });
        nets.push(SafetyNet {
            name: "state_delete_count".into(),
            script: pgv_state_delete_count_probe(),
            redundant_with_mutation: true,
        });
        nets.push(SafetyNet {
            name: "search_empty_table".into(),
            script: pgv_search_empty_probe(),
            redundant_with_mutation: false,
        });

        nets
    }

    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> {
        Some(Box::new(PgVectorIndependentReviewer))
    }

    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();

        // Derive from behavioral_contracts first (highest priority)
        let superseded_names: HashSet<String> = contract
            .behavioral_contracts
            .iter()
            .filter_map(|bc| bc.supersedes.clone())
            .collect();

        for bc in &contract.behavioral_contracts {
            if bc.verification_script.is_empty() {
                continue;
            }
            let check_type = match bc.category {
                BehaviorCategory::StateConsistency => CheckType::CountConsistency,
                BehaviorCategory::SemanticCorrectness => CheckType::ValueRange,
                BehaviorCategory::InterfaceConsistency => CheckType::Idempotency,
                BehaviorCategory::DiagnosticQuality => CheckType::ValueRange,
            };
            checks.push(InvariantCheck {
                name: format!("behavior_{}", bc.name),
                check_type,
                script: bc.verification_script.clone(),
                source: InvariantSource::DerivedFromBehavior,
            });
        }

        // Derive from assertions with keyword matching
        for assertion in &contract.assertions {
            let a_lower = assertion.to_lowercase();

            if a_lower.contains("m") && (a_lower.contains(">=") || a_lower.contains("range") || a_lower.contains("between")) && a_lower.contains("hnsw") {
                checks.push(InvariantCheck {
                    name: "pgv_hnsw_m_range".into(),
                    check_type: CheckType::ValueRange,
                    script: pgv_oracle_hnsw_m_range().script,
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("ef_construction") && (a_lower.contains(">=") || a_lower.contains("at least")) {
                checks.push(InvariantCheck {
                    name: "pgv_ef_construction_range".into(),
                    check_type: CheckType::ValueRange,
                    script: pgv_oracle_ef_construction_range().script,
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("lists") && a_lower.contains(">=") && a_lower.contains("ivfflat") {
                checks.push(InvariantCheck {
                    name: "pgv_ivfflat_lists_range".into(),
                    check_type: CheckType::ValueRange,
                    script: pgv_oracle_ivfflat_lists_range().script,
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("dim") && (a_lower.contains("mismatch") || a_lower.contains("match") || a_lower.contains("dimension")) && a_lower.contains("must") {
                checks.push(InvariantCheck {
                    name: "pgv_dim_mismatch".into(),
                    check_type: CheckType::ValueRange,
                    script: pgv_oracle_dim_mismatch().script,
                    source: InvariantSource::DerivedFromAssertion,
                });
            } else if a_lower.contains("count") && a_lower.contains("consistency") {
                checks.push(InvariantCheck {
                    name: "pgv_insert_count_consistency".into(),
                    check_type: CheckType::CountConsistency,
                    script: pgv_oracle_concurrent_insert_count().script,
                    source: InvariantSource::DerivedFromAssertion,
                });
            }
        }

        // Derive from explicit state_invariants (skip those superseded by behavioral_contracts)
        for si in &contract.state_invariants {
            if superseded_names.contains(&si.name) {
                continue;
            }
            checks.push(InvariantCheck {
                name: si.name.clone(),
                check_type: si.check_type.clone(),
                script: si.assertion_script.clone(),
                source: InvariantSource::ContractExplicit,
            });
        }

        // Always add 5 static PgVector oracle checks
        checks.push(pgv_oracle_hnsw_m_range());
        checks.push(pgv_oracle_ef_construction_range());
        checks.push(pgv_oracle_ivfflat_lists_range());
        checks.push(pgv_oracle_dim_mismatch());
        checks.push(pgv_oracle_concurrent_insert_count());

        // Deduplicate by name (case-insensitive)
        let mut dedup = HashSet::new();
        checks.retain(|c| dedup.insert(c.name.to_lowercase()));

        checks
    }

    fn target_style(&self) -> TargetStyle {
        TargetStyle::PgVector
    }

    fn doc_citation_url(&self) -> String {
        "https://github.com/pgvector/pgvector".to_string()
    }

    fn probe_template(&self) -> &dyn ProbeTemplate {
        &NoopProbeTemplate
    }

    fn db_env(&self) -> Vec<(String, String)> {
        vec![
            ("POSTGRES_PASSWORD".to_string(), "postgres".to_string()),
            ("POSTGRES_DB".to_string(), "testvdb".to_string()),
        ]
    }
}

// PgVector SQL probes — use psycopg2 to connect to PostgreSQL

fn pgv_dim_zero_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_dim0_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
try:
    cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(0))")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] vector(0) column created")
    sys.exit(1)
except Exception as e:
    if "dimensions" in str(e).lower() or "invalid" in str(e).lower() or "at least" in str(e).lower():
        print(f"vector(0) correctly rejected: {e}")
    else:
        print(f"[DEFECT: POOR_DIAGNOSTICS] Unexpected error: {e}")
        sys.exit(1)
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_dim_negative_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_dimneg_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
try:
    cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(-1))")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] vector(-1) column created")
    sys.exit(1)
except Exception as e:
    if "dimensions" in str(e).lower() or "invalid" in str(e).lower():
        print(f"vector(-1) correctly rejected: {e}")
    else:
        print(f"[DEFECT: POOR_DIAGNOSTICS] Unexpected error: {e}")
finally:
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_dim_oversized_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_dimlarge_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
try:
    cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(1000000))")
    conn.commit()
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] vector(1000000) created without upper bound check")
    sys.exit(1)
except Exception as e:
    print(f"vector(1000000) correctly rejected: {e}")
finally:
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_invalid_distance_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_baddist_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding invalid_distance_ops)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] invalid_distance_ops index created")
    sys.exit(1)
except Exception as e:
    print(f"invalid_distance_ops correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_hnsw_m_low_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_hnswm_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding vector_l2_ops) WITH (m = 1)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] hnsw m=1 accepted (should be >= 2)")
    sys.exit(1)
except Exception as e:
    print(f"hnsw m=1 correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_hnsw_ef_construction_low_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_hnsef_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding vector_l2_ops) WITH (ef_construction = 3)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] hnsw ef_construction=3 accepted (should be >= 4)")
    sys.exit(1)
except Exception as e:
    print(f"hnsw ef_construction=3 correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_ivfflat_lists_zero_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_ivf0_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING ivfflat (embedding vector_l2_ops) WITH (lists = 0)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] ivfflat lists=0 accepted (should be >= 1)")
    sys.exit(1)
except Exception as e:
    print(f"ivfflat lists=0 correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_dim_mismatch_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_dimmis_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[1,2,3]')")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] Dim mismatch insert accepted (vector(4) vs [1,2,3])")
    sys.exit(1)
except Exception as e:
    if "dimension" in str(e).lower() or "different" in str(e).lower():
        print(f"dim mismatch correctly rejected: {e}")
    else:
        print(f"[DEFECT: POOR_DIAGNOSTICS] Unexpected error on dim mismatch: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

fn pgv_state_count_probe() -> String {
    r#"
import psycopg2, sys, uuid, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_state_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
for i in range(5):
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count != 5:
    print(f"[DEFECT: STATE_VIOLATION] Insert 5 rows but COUNT={count}")
    sys.exit(1)
print("State count verified: 5 rows")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string()
}

fn pgv_state_delete_count_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_del_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
for i in range(5):
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
cur.execute(f"DELETE FROM {table} WHERE id <= 2")
conn.commit()
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count != 3:
    print(f"[DEFECT: STATE_VIOLATION] Insert 5, delete 2, COUNT={count}")
    sys.exit(1)
print("Delete count verified: 3 remaining")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string()
}

fn pgv_search_empty_probe() -> String {
    r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_empty_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"SELECT * FROM {table} ORDER BY embedding <-> '[1,2,3,4]' LIMIT 5")
    rows = cur.fetchall()
    if len(rows) != 0:
        print(f"[DEFECT: STATE_VIOLATION] Empty table search returned {len(rows)} rows")
        sys.exit(1)
    print("Empty table search returns 0 rows")
except Exception as e:
    print(f"[DEFECT: RUNTIME_FAILURE] Empty table search crashed: {e}")
    sys.exit(1)
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string()
}

// ── PgVector Oracle check scripts ──

fn pgv_oracle_hnsw_m_range() -> InvariantCheck {
    InvariantCheck {
        name: "pgv_hnsw_m_range".into(),
        check_type: CheckType::ValueRange,
        script: r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "oracle_hnswm_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
defects = []
# Test m=1 (below min 2)
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding vector_l2_ops) WITH (m = 1)")
    conn.commit()
    defects.append("m=1 accepted (should be >= 2)")
except Exception as e:
    conn.rollback()
    pass
# Test m=65 (above max 64)
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding vector_l2_ops) WITH (m = 65)")
    conn.commit()
    defects.append("m=65 accepted (should be <= 64)")
except Exception as e:
    conn.rollback()
    pass
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
if defects:
    print(f"[DEFECT: ILLEGAL_SUCCESS] HNSW m range violations: {'; '.join(defects)}")
    sys.exit(1)
print("HNSW m range [2,64] correctly enforced")
"#.to_string(),
        source: InvariantSource::DerivedFromAssertion,
    }
}

fn pgv_oracle_ef_construction_range() -> InvariantCheck {
    InvariantCheck {
        name: "pgv_ef_construction_range".into(),
        check_type: CheckType::ValueRange,
        script: r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "oracle_efc_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING hnsw (embedding vector_l2_ops) WITH (ef_construction = 3)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] hnsw ef_construction=3 accepted (should be >= 4)")
    sys.exit(1)
except Exception as e:
    print(f"hnsw ef_construction=3 correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string(),
        source: InvariantSource::DerivedFromAssertion,
    }
}

fn pgv_oracle_ivfflat_lists_range() -> InvariantCheck {
    InvariantCheck {
        name: "pgv_ivfflat_lists_range".into(),
        check_type: CheckType::ValueRange,
        script: r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "oracle_ivf_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"CREATE INDEX ON {table} USING ivfflat (embedding vector_l2_ops) WITH (lists = 0)")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] ivfflat lists=0 accepted (should be >= 1)")
    sys.exit(1)
except Exception as e:
    print(f"ivfflat lists=0 correctly rejected: {e}")
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string(),
        source: InvariantSource::DerivedFromAssertion,
    }
}

fn pgv_oracle_dim_mismatch() -> InvariantCheck {
    InvariantCheck {
        name: "pgv_dim_mismatch".into(),
        check_type: CheckType::ValueRange,
        script: r#"
import psycopg2, sys, uuid
DB = "{{TESTVDB_DB_URL}}"
table = "oracle_dimmis_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
try:
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[1,2,3]')")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] Dim mismatch insert accepted (vector(4) vs [1,2,3])")
    sys.exit(1)
except Exception as e:
    if "dimension" in str(e).lower() or "different" in str(e).lower():
        print(f"dim mismatch correctly rejected: {e}")
    else:
        print(f"[DEFECT: POOR_DIAGNOSTICS] Unexpected error on dim mismatch: {e}")
        sys.exit(1)
finally:
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
"#.to_string(),
        source: InvariantSource::DerivedFromAssertion,
    }
}

fn pgv_oracle_concurrent_insert_count() -> InvariantCheck {
    InvariantCheck {
        name: "pgv_concurrent_insert_count".into(),
        check_type: CheckType::CountConsistency,
        script: r#"
import psycopg2, sys, uuid, threading
DB = "{{TESTVDB_DB_URL}}"
table = "oracle_conc_" + uuid.uuid4().hex[:8]
N = 10
errors = []

def insert_rows(start, count):
    try:
        conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = conn.cursor()
        for i in range(start, start + count):
            cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        conn.commit()
        conn.close()
    except Exception as e:
        errors.append(str(e))

conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()

threads = []
for t in range(2):
    th = threading.Thread(target=insert_rows, args=(t * (N // 2), N // 2))
    threads.append(th)
    th.start()
for th in threads:
    th.join()

if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Concurrent insert errors: {errors}")
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.commit()
    conn.close()
    sys.exit(1)

cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count != N:
    print(f"[DEFECT: STATE_VIOLATION] Concurrent insert {N} rows but COUNT={count}")
    sys.exit(1)
print(f"Concurrent insert count verified: {N} rows")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        source: InvariantSource::DerivedFromState,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{BehavioralContract, StateInvariant};

    #[test]
    fn test_pgvector_derive_oracle_checks_returns_nonempty() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "create_index".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        assert!(checks.len() >= 5, "expected >= 5 oracle checks, got {}", checks.len());
        for check in &checks {
            assert!(!check.script.is_empty(), "oracle check '{}' has empty script", check.name);
        }
    }

    #[test]
    fn test_pgvector_derive_oracle_checks_from_behavioral_contracts() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "insert".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![
                BehavioralContract {
                    name: "insert_count_consistency".to_string(),
                    category: BehaviorCategory::StateConsistency,
                    endpoints: vec!["INSERT".to_string()],
                    precondition_script: "table exists".to_string(),
                    verification_script: "print('verify count')".to_string(),
                    expected_outcome: "count == N".to_string(),
                    supersedes: None,
                    mutation_rules: vec![],
                },
            ],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        let behavioral: Vec<_> = checks.iter().filter(|c| c.source == InvariantSource::DerivedFromBehavior).collect();
        assert!(behavioral.len() >= 1, "expected >= 1 behavioral check, got {}", behavioral.len());
        assert!(behavioral.iter().any(|c| c.name == "behavior_insert_count_consistency"));
        assert!(behavioral.iter().all(|c| !c.script.is_empty()));
    }

    #[test]
    fn test_pgvector_derive_oracle_checks_from_assertions() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "create_index".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec!["hnsw m must be >= 2".to_string(), "ef_construction must be >= 4".to_string()],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        assert!(checks.iter().any(|c| c.name == "pgv_hnsw_m_range" && !c.script.is_empty()));
        assert!(checks.iter().any(|c| c.name == "pgv_ef_construction_range" && !c.script.is_empty()));
    }

    #[test]
    fn test_pgvector_behavioral_supersedes_state_invariant() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "insert".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![StateInvariant {
                name: "count_check".to_string(),
                check_type: CheckType::CountConsistency,
                endpoint: "SELECT COUNT".to_string(),
                precondition: "table exists".to_string(),
                assertion_script: "print('old check')".to_string(),
            }],
            behavioral_contracts: vec![BehavioralContract {
                name: "insert_count_consistency".to_string(),
                category: BehaviorCategory::StateConsistency,
                endpoints: vec!["INSERT".to_string()],
                precondition_script: "table exists".to_string(),
                verification_script: "print('new check')".to_string(),
                expected_outcome: "count == N".to_string(),
                supersedes: Some("count_check".to_string()),
                mutation_rules: vec![],
            }],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        assert!(checks.iter().any(|c| c.name == "behavior_insert_count_consistency"));
        assert!(!checks.iter().any(|c| c.name == "count_check"));
    }

    #[test]
    fn test_pgvector_behavioral_empty_script_skipped() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "insert".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![BehavioralContract {
                name: "no_script_check".to_string(),
                category: BehaviorCategory::StateConsistency,
                endpoints: vec![],
                precondition_script: String::new(),
                verification_script: String::new(),
                expected_outcome: String::new(),
                supersedes: None,
                mutation_rules: vec![],
            }],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        let behavioral: Vec<_> = checks.iter().filter(|c| c.source == InvariantSource::DerivedFromBehavior).collect();
        assert!(behavioral.is_empty(), "empty verification_script should be skipped");
    }

    #[test]
    fn test_pgvector_oracle_check_names() {
        let plugin = PgVectorPlugin;
        let contract = StructuredContract {
            api_endpoint: "create_index".to_string(),
            doc_url: "https://github.com/pgvector/pgvector".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let checks = plugin.derive_oracle_checks(&contract);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"pgv_hnsw_m_range"), "missing pgv_hnsw_m_range");
        assert!(names.contains(&"pgv_ef_construction_range"), "missing pgv_ef_construction_range");
        assert!(names.contains(&"pgv_ivfflat_lists_range"), "missing pgv_ivfflat_lists_range");
        assert!(names.contains(&"pgv_dim_mismatch"), "missing pgv_dim_mismatch");
        assert!(names.contains(&"pgv_concurrent_insert_count"), "missing pgv_concurrent_insert_count");
    }
}