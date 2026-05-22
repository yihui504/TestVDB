use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::agent::classifier::DefectType;
use crate::sandbox::manager::Sandbox;
use super::{IndependentReviewer, ReviewResult};

pub struct PgVectorIndependentReviewer;

#[async_trait]
impl IndependentReviewer for PgVectorIndependentReviewer {
    fn target_name(&self) -> &str {
        "pgvector"
    }

    async fn run_probe(&self, sandbox: &Sandbox, _port: u16) -> Result<ReviewResult> {
        let db_host = sandbox.db_host.as_ref().unwrap();
        let probe_script = PGVECTOR_REVIEW_PROBE_TEMPLATE.replace("__DB_HOST__", db_host);
        let output = sandbox.exec_script(
            &probe_script,
            &[("TESTVDB_DB_URL", db_host)],
        ).await?;
        if !output.success {
            anyhow::bail!(
                "Independent PgVector probe failed.\nSTDOUT:\n{}\nSTDERR:\n{}",
                output.stdout,
                output.stderr
            );
        }
        let result: Value = serde_json::from_str(output.stdout.trim())?;
        Ok(result)
    }

    fn summarize_findings(&self, probe_json: &ReviewResult) -> Option<(DefectType, Vec<String>)> {
        summarize_pgvector_probe(probe_json)
    }
}

const PGVECTOR_REVIEW_PROBE_TEMPLATE: &str = r#"
import json, psycopg2, uuid, time

DB_HOST = "__DB_HOST__"
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB_HOST} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")

table = "review_pgv_test"
cur.execute(f"DROP TABLE IF EXISTS {table}")
cur.execute(f"CREATE TABLE {table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()

# Insert data
for i in range(5):
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()

# Search
cur.execute(f"SELECT * FROM {table} ORDER BY embedding <-> '[0.1,0.2,0.3,0.4]' LIMIT 3")
search_rows = len(cur.fetchall())

# Dim mismatch insert
dim_mismatch_err = None
try:
    cur.execute(f"INSERT INTO {table} (embedding) VALUES ('[1,2,3]')")
    conn.commit()
except Exception as e:
    dim_mismatch_err = str(e)
conn.rollback()

# Boundary: vector(0) create
dim_zero_table = "review_pgv_dim0"
cur.execute(f"DROP TABLE IF EXISTS {dim_zero_table}")
dim_zero_err = None
try:
    cur.execute(f"CREATE TABLE {dim_zero_table} (id bigserial PRIMARY KEY, embedding vector(0))")
    conn.commit()
except Exception as e:
    dim_zero_err = str(e)
conn.rollback()

# Boundary: vector(-1) create
dim_neg_table = "review_pgv_dimneg"
cur.execute(f"DROP TABLE IF EXISTS {dim_neg_table}")
dim_neg_err = None
try:
    cur.execute(f"CREATE TABLE {dim_neg_table} (id bigserial PRIMARY KEY, embedding vector(-1))")
    conn.commit()
except Exception as e:
    dim_neg_err = str(e)
conn.rollback()

# Boundary: hnsw m=1
hnsw_m_table = "review_pgv_hnswm"
cur.execute(f"DROP TABLE IF EXISTS {hnsw_m_table}")
cur.execute(f"CREATE TABLE {hnsw_m_table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
hnsw_m_err = None
try:
    cur.execute(f"CREATE INDEX ON {hnsw_m_table} USING hnsw (embedding vector_l2_ops) WITH (m = 1)")
    conn.commit()
except Exception as e:
    hnsw_m_err = str(e)
conn.rollback()

# State: count consistency
state_table = "review_pgv_state"
cur.execute(f"DROP TABLE IF EXISTS {state_table}")
cur.execute(f"CREATE TABLE {state_table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
for i in range(5):
    cur.execute(f"INSERT INTO {state_table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
cur.execute(f"SELECT COUNT(*) FROM {state_table}")
state_count = cur.fetchone()[0]

# Cleanup
cur.execute(f"DROP TABLE IF EXISTS {table}")
cur.execute(f"DROP TABLE IF EXISTS {state_table}")
cur.execute(f"DROP TABLE IF EXISTS {hnsw_m_table}")
conn.commit()
conn.close()

print(json.dumps({
    "search_rows": search_rows,
    "dim_mismatch_accepted": dim_mismatch_err is None,
    "dim_zero_accepted": dim_zero_err is None,
    "dim_neg_accepted": dim_neg_err is None,
    "hnsw_m1_accepted": hnsw_m_err is None,
    "state_count": state_count,
}))
"#;

fn summarize_pgvector_probe(probe_value: &Value) -> Option<(DefectType, Vec<String>)> {
    let mut illegal_issues: Vec<String> = Vec::new();

    if probe_value.get("dim_mismatch_accepted").and_then(|v| v.as_bool()).unwrap_or(false) {
        illegal_issues.push("Dimension mismatch insert accepted".to_string());
    }
    if probe_value.get("dim_zero_accepted").and_then(|v| v.as_bool()).unwrap_or(false) {
        illegal_issues.push("vector(0) column created successfully".to_string());
    }
    if probe_value.get("dim_neg_accepted").and_then(|v| v.as_bool()).unwrap_or(false) {
        illegal_issues.push("vector(-1) column created successfully".to_string());
    }
    if probe_value.get("hnsw_m1_accepted").and_then(|v| v.as_bool()).unwrap_or(false) {
        illegal_issues.push("hnsw m=1 index created (should be >= 2)".to_string());
    }

    if !illegal_issues.is_empty() {
        return Some((DefectType::IllegalSuccess, illegal_issues));
    }

    let count = probe_value.get("state_count").and_then(|v| v.as_i64()).unwrap_or(-1);
    if count >= 0 && count != 5 {
        return Some((DefectType::StateLogicViolation,
            vec![format!("Insert 5 rows but COUNT={}", count)]));
    }

    None
}
