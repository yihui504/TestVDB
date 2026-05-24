use super::{SafetyNet, TargetPlugin, TargetStyle};
use crate::agent::oracle::InvariantCheck;
use crate::agent::probe::{ProbeTemplate, NoopProbeTemplate};
use crate::contract::schema::StructuredContract;
use crate::review::IndependentReviewer;
use crate::review::pgvector::PgVectorIndependentReviewer;

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
        for assertion in &contract.assertions {
            checks.push(InvariantCheck {
                name: format!("PgVector invariant: {}", assertion),
                check_type: crate::contract::schema::CheckType::ValueRange,
                script: String::new(),
                source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
            });
        }
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