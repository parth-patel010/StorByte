import sqlite3
import os

p = os.path.expandvars(r"%APPDATA%\Stroex\Stroex\data\metadata.sqlite")
print("path", p)
print("exists", os.path.exists(p), "size", os.path.getsize(p) if os.path.exists(p) else None)
for mode in ["rw", "ro"]:
    try:
        uri = f"file:{p}?mode={mode}"
        c = sqlite3.connect(uri, uri=True, timeout=3)
        c.execute("PRAGMA busy_timeout=3000")
        print("mode", mode)
        print(" accounts", c.execute("select id,email,status,last_indexed_at,quota_bytes from accounts").fetchall())
        print(" nodes", c.execute("select count(*) from nodes").fetchone()[0])
        print(" sample", c.execute("select id,parent_id,name,virtual_path from nodes limit 15").fetchall())
        print(" nonroot", c.execute("select count(*) from nodes where virtual_path != ?", ("\\",)).fetchone()[0])
        c.close()
    except Exception as e:
        print("mode", mode, "ERR", e)
