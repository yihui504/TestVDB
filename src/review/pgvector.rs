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
        let db_host = sandbox.db_host.as_ref().ok_or_else(|| anyhow::anyhow!("sandbox db_host missing"))?;
        let probe_script = PGVECTOR_REVIEW_PROBE_TEMPLATE
            .replace("__DB_HOST__", db_host)
            .replace("__PG_CONNECT__", &crate::target::pgvector::pg_connect_code("DB_HOST"));
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
conn = __PG_CONNECT__
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

# Concurrent: insert + delete + search stale read
import threading
conc_table = "review_pgv_conc"
cur.execute(f"DROP TABLE IF EXISTS {conc_table}")
cur.execute(f"CREATE TABLE {conc_table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
for i in range(10):
    cur.execute(f"INSERT INTO {conc_table} (id, embedding) VALUES ({i}, '[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
conc_errors = []
conc_stale_found = []
def conc_delete():
    try:
        c = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=DB_HOST, port=5432)
        cc = c.cursor()
        cc.execute(f"DELETE FROM {conc_table} WHERE id < 5")
        c.commit()
        c.close()
    except Exception as e:
        conc_errors.append(str(e))
def conc_search():
    try:
        c = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=DB_HOST, port=5432)
        cc = c.cursor()
        cc.execute(f"SELECT id FROM {conc_table} WHERE id < 5 ORDER BY embedding <-> '[0.1,0.2,0.3,0.4]' LIMIT 5")
        results = cc.fetchall()
        c.close()
        if results:
            conc_stale_found.append(len(results))
    except Exception as e:
        conc_errors.append(str(e))
threads = []
for _ in range(5):
    threads.append(threading.Thread(target=conc_delete))
    threads.append(threading.Thread(target=conc_search))
for t in threads:
    t.start()
for t in threads:
    t.join()
time.sleep(0.5)
cur.execute(f"SELECT COUNT(*) FROM {conc_table} WHERE id < 5")
conc_remaining = cur.fetchone()[0]

# Concurrent INSERT + COUNT consistency
conc2_table = "review_pgv_conc2"
cur.execute(f"DROP TABLE IF EXISTS {conc2_table}")
cur.execute(f"CREATE TABLE {conc2_table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
conc2_errors = []
def conc2_insert(thread_id):
    try:
        c = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=DB_HOST, port=5432)
        cc = c.cursor()
        for i in range(5):
            cc.execute(f"INSERT INTO {conc2_table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        c.commit()
        c.close()
    except Exception as e:
        conc2_errors.append(str(e))
threads2 = []
for t_id in range(4):
    threads2.append(threading.Thread(target=conc2_insert, args=(t_id,)))
for t in threads2:
    t.start()
for t in threads2:
    t.join()
cur.execute(f"SELECT COUNT(*) FROM {conc2_table}")
conc2_count = cur.fetchone()[0]

# HNSW index + concurrent search during index build
conc3_table = "review_pgv_conc3"
cur.execute(f"DROP TABLE IF EXISTS {conc3_table}")
cur.execute(f"CREATE TABLE {conc3_table} (id bigserial PRIMARY KEY, embedding vector(4))")
conn.commit()
for i in range(20):
    cur.execute(f"INSERT INTO {conc3_table} (embedding) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
conc3_errors = []
conc3_search_ok = []
def conc3_build_index():
    try:
        c = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=DB_HOST, port=5432)
        cc = c.cursor()
        cc.execute(f"CREATE INDEX IF NOT EXISTS idx_{conc3_table} ON {conc3_table} USING hnsw (embedding vector_l2_ops)")
        c.commit()
        c.close()
    except Exception as e:
        conc3_errors.append(str(e))
def conc3_search():
    try:
        time.sleep(0.1)
        c = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=DB_HOST, port=5432)
        cc = c.cursor()
        cc.execute(f"SELECT id FROM {conc3_table} ORDER BY embedding <-> '[0.5,0.5,0.5,0.5]' LIMIT 3")
        rows = cc.fetchall()
        c.close()
        if len(rows) > 0:
            conc3_search_ok.append(len(rows))
    except Exception as e:
        conc3_errors.append(str(e))
threads3 = []
threads3.append(threading.Thread(target=conc3_build_index))
for _ in range(3):
    threads3.append(threading.Thread(target=conc3_search))
for t in threads3:
    t.start()
for t in threads3:
    t.join()
time.sleep(0.5)

# Cleanup
cur.execute(f"DROP TABLE IF EXISTS {table}")
cur.execute(f"DROP TABLE IF EXISTS {state_table}")
cur.execute(f"DROP TABLE IF EXISTS {hnsw_m_table}")
cur.execute(f"DROP TABLE IF EXISTS {conc_table}")
cur.execute(f"DROP TABLE IF EXISTS {conc2_table}")
cur.execute(f"DROP TABLE IF EXISTS {conc3_table}")
conn.commit()
conn.close()

print(json.dumps({
    "search_rows": search_rows,
    "dim_mismatch_accepted": dim_mismatch_err is None,
    "dim_zero_accepted": dim_zero_err is None,
    "dim_neg_accepted": dim_neg_err is None,
    "hnsw_m1_accepted": hnsw_m_err is None,
    "state_count": state_count,
    "conc_stale_searches": len(conc_stale_found),
    "conc_remaining_after_delete": conc_remaining,
    "conc_errors": len(conc_errors),
    "conc2_4x5_insert_count": conc2_count,
    "conc2_errors": len(conc2_errors),
    "conc3_search_ok": len(conc3_search_ok),
    "conc3_errors": len(conc3_errors),
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

    let conc_remaining = probe_value.get("conc_remaining_after_delete").and_then(|v| v.as_i64()).unwrap_or(-1);
    if conc_remaining > 0 {
        return Some((DefectType::DataCorruption,
            vec![format!("Concurrent 5x DELETE WHERE id<5 should leave 0 rows but COUNT={}", conc_remaining)]));
    }

    let conc_errors = probe_value.get("conc_errors").and_then(|v| v.as_i64()).unwrap_or(0);
    if conc_errors > 0 {
        return Some((DefectType::DataCorruption,
            vec![format!("Concurrent DELETE+SEARCH had {} errors", conc_errors)]));
    }

    let conc2_count = probe_value.get("conc2_4x5_insert_count").and_then(|v| v.as_i64()).unwrap_or(-1);
    if conc2_count >= 0 && conc2_count != 20 {
        return Some((DefectType::StateLogicViolation,
            vec![format!("Concurrent 4x5 INSERT should be 20 rows but COUNT={}", conc2_count)]));
    }
    let conc2_errors = probe_value.get("conc2_errors").and_then(|v| v.as_i64()).unwrap_or(0);
    if conc2_errors > 0 {
        return Some((DefectType::DataCorruption,
            vec![format!("Concurrent 4x INSERT had {} errors", conc2_errors)]));
    }

    let conc3_errors = probe_value.get("conc3_errors").and_then(|v| v.as_i64()).unwrap_or(0);
    if conc3_errors > 0 {
        return Some((DefectType::DataCorruption,
            vec![format!("Concurrent HNSW index build + search had {} errors", conc3_errors)]));
    }
    let conc3_search_ok = probe_value.get("conc3_search_ok").and_then(|v| v.as_i64()).unwrap_or(0);
    if conc3_search_ok == 0 {
        return Some((DefectType::StateLogicViolation,
            vec!["Concurrent search during HNSW index build returned 0 results".to_string()]));
    }

    None
}
