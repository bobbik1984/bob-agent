import sqlite3
import json
import sys
import os

sys.stdout.reconfigure(encoding='utf-8')

db_path = os.path.expanduser(r'~/AppData/Roaming/bob-agent/bob.db')

db = sqlite3.connect(db_path)
c = db.cursor()
c.execute("SELECT id, label, node_type, source, metadata FROM kg_nodes WHERE node_type = 'Project' OR node_type = 'project'")
for row in c.fetchall():
    print(row)
