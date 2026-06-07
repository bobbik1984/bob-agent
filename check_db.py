import sqlite3

db_path = r'C:\Users\xm_bo\AppData\Roaming\bob-agent\bob.db'
try:
    conn = sqlite3.connect(db_path)
    cur = conn.cursor()
    
    cur.execute("INSERT OR REPLACE INTO kv_store (key, value) VALUES ('last_dream_timestamp', '1700000000')")
    conn.commit()
    print('? Successfully reset last_dream_timestamp to past.')
    
except Exception as e:
    print(f'Error: {e}')
