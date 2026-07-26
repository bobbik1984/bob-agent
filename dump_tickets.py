import sqlite3
import os
import json

db_path = os.path.expandvars(r"%APPDATA%\bob.agent\bob.db")
conn = sqlite3.connect(db_path)
rows = conn.cursor().execute("SELECT id, label, metadata FROM kg_nodes WHERE node_type='Ticket' OR node_type='ticket'").fetchall()
for r in rows:
    print(f"ID: {r[0]}")
    print(f"Label: {r[1]}")
    print(f"Meta: {r[2]}")
    print("---")
conn.close()
