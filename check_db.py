import sqlite3
import os

db_path = os.path.expandvars(r"%APPDATA%\bob.agent\bob.db")
conn = sqlite3.connect(db_path)
print(conn.cursor().execute("SELECT COUNT(*) FROM kg_nodes WHERE node_type='Ticket' OR node_type='ticket'").fetchone()[0])
conn.close()
