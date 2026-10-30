import sqlite3, os, sys

db = os.path.join(os.environ['APPDATA'], 'com.clipvault.app', 'clips.db')
uri_db = 'file:' + db.replace(os.sep, '/') + '?mode=ro'
print('DB_RO_URI=' + uri_db)
con = sqlite3.connect(uri_db, uri=True)
try:
    cur = con.cursor()
    tables = [r[0] for r in cur.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()]
    print('TABLES=' + ','.join(tables))
    for t in tables:
        try:
            n = cur.execute('SELECT COUNT(*) FROM "%s"' % t).fetchone()[0]
            print('ROWS %s = %d' % (t, n))
        except Exception as e:
            print('ROWS %s ERROR %s' % (t, e))
finally:
    con.close()
print('ROWCOUNT_DONE_READONLY')
