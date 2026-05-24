import psycopg2, threading, uuid, time, sys, os

DB = 'localhost'
results = {}

# === Probe 1: Concurrent insert count ===
table = 't_conc_' + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
conn.commit()
errors = []

def insert_batch(start, count):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur2 = c.cursor()
        for i in range(start, start+count):
            cur2.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        c.commit()
        c.close()
    except Exception as e:
        errors.append(str(e))

threads = [threading.Thread(target=insert_batch, args=(i*10,10)) for i in range(4)]
for t in threads: t.start()
for t in threads: t.join()
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
results['concurrent_insert'] = f"expected=40 got={count} errors={len(errors)} {'OK' if count==40 and not errors else 'DEFECT'}"

cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()

# === Probe 2: Concurrent upsert same ID ===
table2 = 't_cups_' + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table2} (id int PRIMARY KEY, emb vector(4))")
conn.commit()
errors2 = []

def upsert_id(id_val):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur2 = c.cursor()
        cur2.execute(f"INSERT INTO {table2} (id, emb) VALUES ({id_val}, '[{0.1*id_val},{0.2*id_val},{0.3*id_val},{0.4*id_val}]') ON CONFLICT (id) DO UPDATE SET emb = EXCLUDED.emb")
        c.commit()
        c.close()
    except Exception as e:
        errors2.append(str(e))

threads2 = [threading.Thread(target=upsert_id, args=(1,)) for _ in range(10)]
for t in threads2: t.start()
for t in threads2: t.join()
time.sleep(0.3)
cur.execute(f"SELECT COUNT(*) FROM {table2}")
count2 = cur.fetchone()[0]
results['concurrent_upsert'] = f"expected=1 got={count2} errors={len(errors2)} {'OK' if count2==1 and not errors2 else 'DEFECT'}"

cur.execute(f"DROP TABLE IF EXISTS {table2}")
conn.commit()
conn.close()

# === Probe 3: Concurrent create/drop cycle ===
errors3 = []

def create_drop_cycle():
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur2 = c.cursor()
        name = 't_cd_' + uuid.uuid4().hex[:8]
        cur2.execute(f"CREATE TABLE {name} (id serial PRIMARY KEY, emb vector(4))")
        cur2.execute(f"INSERT INTO {name} (emb) VALUES ('[1,2,3,4]')")
        c.commit()
        cur2.execute(f"DROP TABLE {name}")
        c.commit()
        c.close()
    except Exception as e:
        errors3.append(str(e))

threads3 = [threading.Thread(target=create_drop_cycle) for _ in range(20)]
for t in threads3: t.start()
for t in threads3: t.join()
results['create_drop_cycle'] = f"errors={len(errors3)} {'OK' if not errors3 else 'DEFECT: ' + str(errors3[:3])}"

# === Probe 4: Rapid inserts ===
table4 = 't_rapid_' + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table4} (id serial PRIMARY KEY, emb vector(4))")
conn.commit()
errors4 = 0
for i in range(500):
    try:
        cur.execute(f"INSERT INTO {table4} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        if i % 100 == 0:
            conn.commit()
    except Exception as e:
        errors4 += 1
conn.commit()
cur.execute(f"SELECT COUNT(*) FROM {table4}")
count4 = cur.fetchone()[0]
results['rapid_inserts'] = f"expected=500 got={count4} errors={errors4} {'OK' if count4==500 and errors4==0 else 'DEFECT'}"

cur.execute(f"DROP TABLE IF EXISTS {table4}")
conn.commit()
conn.close()

for k, v in results.items():
    print(f"{k}: {v}")
