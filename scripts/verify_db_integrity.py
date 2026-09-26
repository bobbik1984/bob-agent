#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Safe SQLite Database Integrity Verifier
---------------------------------------
Verifies SQLite database health via PRAGMA integrity_check.
Strictly prevents false-positive empty database creation:
1. Asserts target file exists and has non-zero byte size.
2. Connects using read-only URI mode (mode=ro) so SQLite will never create a missing file.
3. Asserts PRAGMA integrity_check output is strictly [('ok',)].
"""

import os
import sys
import pathlib
import sqlite3

def verify_db_integrity(db_path_str: str) -> bool:
    try:
        p = pathlib.Path(db_path_str).resolve()
    except Exception as e:
        sys.stderr.write(f"ERROR: Invalid path syntax '{db_path_str}': {e}\n")
        return False

    # 1. 前置存在性与非空校验：严禁对不存在或 0 字节文件放行
    if not p.is_file():
        sys.stderr.write(f"ERROR: Database file does not exist: {p}\n")
        return False

    try:
        file_size = p.stat().st_size
    except Exception as e:
        sys.stderr.write(f"ERROR: Unable to stat file '{p}': {e}\n")
        return False

    if file_size == 0:
        sys.stderr.write(f"ERROR: Database file is 0 bytes (empty/uninitialized): {p}\n")
        return False

    # 2. 只读 URI 连接：mode=ro 阻止任何写入，且目标缺失时绝不自动新建文件
    uri = f"{p.as_uri()}?mode=ro"
    con = None
    try:
        con = sqlite3.connect(uri, uri=True)
        cursor = con.execute("PRAGMA integrity_check;")
        rows = cursor.fetchall()
        con.close()
        con = None

        # 3. 严格结果判等：必须严格仅有一行且其值为 'ok'
        if rows == [("ok",)]:
            print("ok")
            return True
        else:
            sys.stderr.write(f"ERROR: PRAGMA integrity_check failed with corruption report: {rows}\n")
            return False
    except sqlite3.OperationalError as e:
        sys.stderr.write(f"ERROR: SQLite OperationalError (unable to open or read database): {e}\n")
        return False
    except sqlite3.DatabaseError as e:
        sys.stderr.write(f"ERROR: SQLite DatabaseError (file is corrupted or not a database): {e}\n")
        return False
    except Exception as e:
        sys.stderr.write(f"ERROR: Unexpected exception during integrity check: {e}\n")
        return False
    finally:
        if con is not None:
            try:
                con.close()
            except Exception:
                pass

if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.stderr.write("Usage: python verify_db_integrity.py <path_to_db>\n")
        sys.exit(2)

    target_path = sys.argv[1]
    if verify_db_integrity(target_path):
        sys.exit(0)
    else:
        sys.exit(1)
