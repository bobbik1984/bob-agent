import sqlite3
import json
import sys

# Ensure utf-8 encoding for output
sys.stdout.reconfigure(encoding='utf-8')

db = sqlite3.connect(r"C:\Users\xm_bo\AppData\Roaming\bob-agent\bob.db")
c = db.cursor()
c.execute("SELECT content FROM messages WHERE role='assistant' ORDER BY created_at DESC LIMIT 5")
rows = c.fetchall()
with open("db_output.txt", "w", encoding="utf-8") as f:
    for row in rows:
        f.write(repr(row[0]) + "\n-----\n")
