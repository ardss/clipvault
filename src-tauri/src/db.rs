use rusqlite::Connection;
use std::sync::Mutex;
use tauri::Manager;

pub struct Db(pub Mutex<Connection>);

#[derive(Clone, serde::Serialize)]
pub struct Clip {
    pub id: i64,
    pub kind: String, // "text" | "image"
    pub preview: String,
    pub image_path: Option<String>,
    pub content: Option<String>,
    pub pinned: bool,
    pub use_count: i64,
    pub created_at: i64,
    pub text_path: Option<String>,
}

pub fn init(app: &tauri::App) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("images")).map_err(|e| e.to_string())?;
    // a corrupt db must not brick the app: the whole setup (DDL probes a
    // malformed image, quick_check catches subtler btree damage) runs through
    // the quarantine-and-retry path
    let setup = |conn: &Connection| -> Result<(), String> { setup_schema(conn) };
    let conn = match open_db(&dir).and_then(|c| setup(&c).map(|_| c)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[cv] db init failed ({e}) — quarantining corrupt database");
            quarantine_db(&dir)?;
            let c = open_db(&dir).map_err(|e2| format!("db unusable even after recreate: {e2}"))?;
            setup(&c).map_err(|e2| format!("fresh db failed setup: {e2}"))?;
            c
        }
    };
    app.manage(Db(Mutex::new(conn)));
    Ok(())
}

fn setup_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS clips(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           kind TEXT NOT NULL,
           content TEXT,
           image_path TEXT,
           preview TEXT NOT NULL,
           pinned INTEGER NOT NULL DEFAULT 0,
           use_count INTEGER NOT NULL DEFAULT 0,
           created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(pinned, created_at);",
    )
    .map_err(|e| e.to_string())?;
    // migrations: html blob for rich text, files JSON for CF_HDROP entries
    let has_col = |name: &str| -> bool {
        conn.prepare("PRAGMA table_info(clips)")
            .map(|mut st| {
                st.query_map([], |r| r.get::<_, String>(1))
                    .map(|rows| rows.flatten().any(|c| c == name))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    };
    if !has_col("html") {
        conn.execute_batch("ALTER TABLE clips ADD COLUMN html BLOB;")
            .map_err(|e| e.to_string())?;
    }
    if !has_col("files") {
        conn.execute_batch("ALTER TABLE clips ADD COLUMN files TEXT;")
            .map_err(|e| e.to_string())?;
    }
    if !has_col("text_path") {
        // oversized texts live in a side file; the row keeps a short preview
        conn.execute_batch("ALTER TABLE clips ADD COLUMN text_path TEXT;")
            .map_err(|e| e.to_string())?;
    }
    // dedup lookups run on EVERY capture — without these they are full table
    // scans over inline content (up to 256KB per row): O(n) per copy
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content) WHERE content IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_image_path ON clips(image_path) WHERE image_path IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_text_path ON clips(text_path) WHERE text_path IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_files ON clips(files) WHERE files IS NOT NULL;",
    )
    .map_err(|e| e.to_string())?;
    // silent btree corruption passes DDL — verify before serving queries
    let ok = conn
        .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "error".into());
    if ok != "ok" {
        return Err(format!("db failed quick_check: {ok}"));
    }
    Ok(())
}

fn open_db(dir: &std::path::Path) -> Result<Connection, String> {
    Connection::open(dir.join("clips.db")).map_err(|e| e.to_string())
}

fn quarantine_db(dir: &std::path::Path) -> Result<(), String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let db = dir.join("clips.db");
    if db.exists() {
        std::fs::rename(&db, dir.join(format!("clips.db.corrupt-{ts}")))
            .map_err(|e| e.to_string())?;
    }
    for f in ["clips.db-wal", "clips.db-shm"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    Ok(())
}

fn row_to_clip(r: &rusqlite::Row) -> rusqlite::Result<Clip> {
    Ok(Clip {
        id: r.get(0)?,
        kind: r.get(1)?,
        preview: r.get(2)?,
        image_path: r.get(3)?,
        content: r.get(4)?,
        pinned: r.get::<_, i64>(5)? != 0,
        use_count: r.get(6)?,
        created_at: r.get(7)?,
        text_path: r.get(8)?,
    })
}

const COLS: &str =
    "id, kind, preview, image_path, substr(content, 1, 4096) AS content, pinned, use_count, created_at, text_path";
/// texts at or above this length MAY be truncated by the list query — the
/// zoom window re-fetches the full body via clip_content. 4096 caps the
/// refresh payload (~2 MB at 500 rows) while previews never exceed it
pub fn list(conn: &Connection, filter: &str, query: &str) -> Result<Vec<Clip>, String> {
    cvlog!(
        "[cv] list: filter={filter:?} query={query:?} total={}",
        if std::env::var("CV_LOG").is_ok() {
            conn.query_row("SELECT count(*) FROM clips", [], |r| r.get::<_, i64>(0))
                .unwrap_or(-1)
        } else {
            -1
        }
    );
    let base = match filter {
        "text" => " AND kind IN ('text','html')",
        "image" => " AND kind='image'",
        "file" => " AND kind='file'",
        "link" => " AND kind='text' AND (preview LIKE 'http://%' OR preview LIKE 'https://%')",
        _ => "",
    };
    // escape LIKE wildcards in user input, bind as a parameter
    let like = format!(
        "%{}%",
        query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let sql = if query.is_empty() {
        format!(
            "SELECT {COLS} FROM clips WHERE 1=1{base}{} ORDER BY pinned DESC, created_at DESC LIMIT 500",
            if filter == "pinned" { " AND pinned=1" } else { "" }
        )
    } else {
        format!(
            "SELECT * FROM (
               SELECT {COLS} FROM clips WHERE pinned=1{base} AND preview LIKE ?1 ESCAPE '\\'
               UNION ALL
               SELECT {COLS} FROM clips WHERE pinned=0{base} AND preview LIKE ?1 ESCAPE '\\'
             ) ORDER BY pinned DESC, created_at DESC LIMIT 500"
        )
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = if query.is_empty() {
        // the no-search SQL has no placeholder — binding one would error out
        stmt.query_map([], row_to_clip)
    } else {
        stmt.query_map([&like], row_to_clip)
    }
    .map_err(|e| e.to_string())?;
    let mapped: Vec<Result<Clip, rusqlite::Error>> = rows.collect();
    let out: Vec<Clip> = mapped
        .iter()
        .filter_map(|r| r.as_ref().ok().cloned())
        .collect();
    let dropped = mapped.len() - out.len();
    if dropped > 0 {
        // silent row drops are the symptom of btree corruption — surface it
        cvlog!("[cv] list: {dropped} rows unreadable (possible db corruption)");
    }
    Ok(out)
}

pub fn upsert_text(conn: &Connection, content: &str) -> Result<Option<i64>, String> {
    let mut preview: String = content.chars().take(300).collect();
    if content.len() > 300 {
        preview.push('…');
    }
    if preview.trim().is_empty() {
        return Ok(None);
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // same content may already exist as 'text' or as 'html' (rich copy) —
    // user-visible rule: one entry per unique content
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM clips WHERE content=?1 AND kind IN ('text','html')",
            [content],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .unwrap_or(None)
    {
        tx.execute(
            "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
            rusqlite::params![id, now()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(Some(id));
    }
    tx.execute(
        "INSERT INTO clips(kind, content, preview, pinned, use_count, created_at) VALUES('text',?1,?2,0,1,?3)",
        rusqlite::params![content, preview, now()],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(id))
}

/// On-disk file paths owned by an evicted row.
pub type EvictedFiles = (Option<String>, Option<String>);

pub fn enforce_limit(conn: &Connection, max: i64) -> Result<Vec<EvictedFiles>, String> {
    // a hand-edited limit of 0/negative must not become "delete everything"
    // (SQLite treats LIMIT -1 as unbounded and clamps negative OFFSET to 0)
    if max <= 0 {
        return Ok(Vec::new());
    }
    // find evicted rows (oldest unpinned beyond the limit), collect their image
    // and oversized-text side files, then delete the rows — one transaction so
    // a crash can't orphan files for rows that still exist (or vice versa)
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let victims: Vec<(Option<String>, Option<String>)> = {
        let mut stmt = tx
            .prepare(
                "SELECT image_path, text_path FROM clips WHERE pinned=0 ORDER BY created_at DESC LIMIT -1 OFFSET ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([max], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    tx.execute(
        "DELETE FROM clips WHERE id IN (
           SELECT id FROM clips WHERE pinned=0 ORDER BY created_at DESC LIMIT -1 OFFSET ?1)",
        [max],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(victims)
}

pub fn upsert_image(conn: &Connection, path: &str, w: u32, h: u32) -> Result<Option<i64>, String> {
    let preview = format!("[Image {w}x{h}]");
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM clips WHERE kind='image' AND image_path=?1",
            [path],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .unwrap_or(None)
    {
        tx.execute(
            "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
            rusqlite::params![id, now()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(Some(id));
    }
    tx.execute(
        "INSERT INTO clips(kind, image_path, preview, pinned, use_count, created_at) VALUES('image',?1,?2,0,1,?3)",
        rusqlite::params![path, preview, now()],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(id))
}

pub fn upsert_text_file(
    conn: &Connection,
    preview: &str,
    path: &str,
) -> Result<Option<i64>, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let dup: Option<i64> = tx
        .query_row("SELECT id FROM clips WHERE text_path=?1", [path], |r| {
            r.get::<_, i64>(0)
        })
        .map(Some)
        .unwrap_or(None);
    if let Some(id) = dup {
        tx.execute(
            "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
            rusqlite::params![id, now()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(Some(id));
    }
    tx.execute(
        "INSERT INTO clips(kind, content, text_path, preview, use_count, created_at) VALUES('text', ?1, ?2, ?3, 1, ?4)",
        rusqlite::params![preview, path, preview, now()],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(id))
}

pub fn toggle_pin(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("UPDATE clips SET pinned = 1 - pinned WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Collects the on-disk files of every row, then deletes all rows in one
/// transaction. Files are removed by the caller (which holds no lock).
pub fn clear_all(conn: &Connection) -> Result<Vec<EvictedFiles>, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let victims: Vec<EvictedFiles> = {
        let mut stmt = tx
            .prepare("SELECT image_path, text_path FROM clips")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    // "cleared means gone": zero deleted pages and truncate the WAL, or
    // plaintext clipboard history survives on disk inside the -wal file
    conn.pragma_update(None, "secure_delete", true)
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM clips", [])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    // both pragmas return rows — execute/query them, not execute_batch
    let _: Result<(), _> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let _ = conn.execute("VACUUM", []);
    let _ = conn.pragma_update(None, "secure_delete", false);
    Ok(victims)
}

/// Deletes a clip row and its on-disk files: the PNG, its `_t.png` thumbnail
/// and, for oversized text entries, the full-text side file.
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let (image_path, text_path): (Option<String>, Option<String>) = tx
        .query_row(
            "SELECT image_path, text_path FROM clips WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .unwrap_or((None, None));
    tx.execute("DELETE FROM clips WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    remove_clip_files(image_path.as_deref(), text_path.as_deref());
    Ok(())
}

/// Removes an image, its `_t.png` thumbnail and an oversized-text side file,
/// ignoring missing files.
pub fn remove_clip_files(image_path: Option<&str>, text_path: Option<&str>) {
    if let Some(p) = image_path {
        let _ = std::fs::remove_file(p);
        if let Some(stem) = std::path::Path::new(p).file_stem() {
            let thumb =
                std::path::Path::new(p).with_file_name(format!("{}_t.png", stem.to_string_lossy()));
            let _ = std::fs::remove_file(thumb);
        }
    }
    if let Some(p) = text_path {
        let _ = std::fs::remove_file(p);
    }
}

pub fn get_clip(
    conn: &Connection,
    id: i64,
) -> Result<(String, Option<String>, Option<String>), String> {
    conn.query_row(
        "SELECT kind, content, image_path FROM clips WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .map_err(|e| e.to_string())
}

/// Full body of a text/html row — the list query truncates content at
/// 20000 chars; the zoom window calls this when it needs the rest.
pub fn get_content(conn: &Connection, id: i64) -> Result<String, String> {
    conn.query_row("SELECT content FROM clips WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| e.to_string())
}

pub fn mark_used(conn: &Connection, id: i64) {
    let _ = conn.execute(
        "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
        rusqlite::params![id, now()],
    );
}

fn now() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static LAST: AtomicI64 = AtomicI64::new(0);
    // wall clock can jump backward (NTP correction, manual change); ordering
    // and OFFSET-based pruning rely on created_at moving forward
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut t = wall;
    loop {
        let last = LAST.load(Ordering::SeqCst);
        if t <= last {
            t = last.saturating_add(1);
        }
        if LAST
            .compare_exchange(last, t, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return t;
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct Stats {
    pub total: i64,
    pub text: i64,
    pub image: i64,
    pub link: i64,
    pub file: i64,
    pub pinned: i64,
    pub today: i64,
    pub paste_total: i64,
    pub daily: Vec<(String, i64)>, // (YYYY-MM-DD, count) last 7 days
    pub top: Vec<(String, i64)>,   // (preview, use_count) top 5
}

pub fn stats(conn: &Connection) -> Result<Stats, String> {
    let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    let today = one(&format!(
        "SELECT count(*) FROM clips WHERE created_at >= {}",
        now() - 86_400_000
    ));
    let mut daily = Vec::new();
    for d in (0..7).rev() {
        let lo = now() - (d + 1) * 86_400_000;
        let hi = now() - d * 86_400_000;
        let c: i64 = conn
            .query_row(
                "SELECT count(*) FROM clips WHERE created_at > ?1 AND created_at <= ?2",
                rusqlite::params![lo, hi],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let day = chrono_date(hi);
        daily.push((day, c));
    }
    let mut top = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT preview, use_count FROM clips ORDER BY use_count DESC, created_at DESC LIMIT 5",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?;
    for r in rows.flatten() {
        top.push(r);
    }
    Ok(Stats {
        total: one("SELECT count(*) FROM clips"),
        text: one("SELECT count(*) FROM clips WHERE kind='text'"),
        image: one("SELECT count(*) FROM clips WHERE kind='image'"),
        link: one("SELECT count(*) FROM clips WHERE kind='text' AND (preview LIKE 'http://%' OR preview LIKE 'https://%')"),
        file: one("SELECT count(*) FROM clips WHERE kind='file'"),
        pinned: one("SELECT count(*) FROM clips WHERE pinned=1"),
        today,
        paste_total: one("SELECT COALESCE(sum(use_count),0) FROM clips"),
        daily,
        top,
    })
}

fn chrono_date(ms: i64) -> String {
    // days since epoch -> YYYY-MM-DD (civil algorithm)
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn upsert_file(conn: &Connection, paths: &[String]) -> Result<Option<i64>, String> {
    if paths.is_empty() {
        return Ok(None);
    }
    let joined = paths.join("|");
    let preview = if paths.len() == 1 {
        let p = std::path::Path::new(&paths[0]);
        format!(
            "[File] {}",
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| paths[0].clone())
        )
    } else {
        format!("[File] {} +{} more", paths[0], paths.len() - 1)
    };
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM clips WHERE kind='file' AND files=?1",
            [&joined],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .unwrap_or(None)
    {
        tx.execute(
            "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
            rusqlite::params![id, now()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(Some(id));
    }
    tx.execute(
        "INSERT INTO clips(kind, files, preview, use_count, created_at) VALUES('file',?1,?2,1,?3)",
        rusqlite::params![joined, preview, now()],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(id))
}

pub fn upsert_html(conn: &Connection, plain: &str, html: &[u8]) -> Result<Option<i64>, String> {
    if plain.trim().is_empty() {
        return Ok(None);
    }
    let mut preview: String = plain.chars().take(300).collect();
    if plain.len() > 300 {
        preview.push('…');
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // a plain-text entry with the same content gets upgraded to rich text
    let dup: Option<i64> = tx
        .query_row(
            "SELECT id FROM clips WHERE content=?1 AND kind IN ('text','html')",
            [plain],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .unwrap_or(None);
    if let Some(id) = dup {
        tx.execute(
            "UPDATE clips SET kind='html', use_count=use_count+1, created_at=?2, html=?3 WHERE id=?1",
            rusqlite::params![id, now(), html],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(Some(id));
    }
    tx.execute(
        "INSERT INTO clips(kind, content, html, preview, use_count, created_at) VALUES('html',?1,?2,?3,1,?4)",
        rusqlite::params![plain, html, preview, now()],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(id))
}

pub fn get_files(conn: &Connection, id: i64) -> Result<Vec<String>, String> {
    let raw: Option<String> = conn
        .query_row("SELECT files FROM clips WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(raw
        .map(|s| s.split('|').map(|p| p.to_string()).collect())
        .unwrap_or_default())
}

pub fn get_html(conn: &Connection, id: i64) -> Result<(String, Vec<u8>), String> {
    conn.query_row(
        "SELECT content, COALESCE(html, x'') FROM clips WHERE id=?1",
        [id],
        |r| {
            let c: String = r.get(0)?;
            let h: Vec<u8> = r.get(1)?;
            Ok((c, h))
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=MEMORY;
             CREATE TABLE clips(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               kind TEXT NOT NULL,
               content TEXT,
               image_path TEXT,
               html BLOB,
               files TEXT,
               preview TEXT NOT NULL,
               pinned INTEGER NOT NULL DEFAULT 0,
               use_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               text_path TEXT
             );
             CREATE INDEX idx_clips_pinned ON clips(pinned, created_at);",
        )
        .unwrap();
        conn
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
            .unwrap()
    }

    fn bulk_insert_text(conn: &Connection, n: usize, base_ms: i64) {
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..n {
            conn.execute(
                "INSERT INTO clips(kind, content, preview, pinned, use_count, created_at) VALUES('text',?1,?2,0,1,?3)",
                rusqlite::params![format!("stress text {i}"), format!("stress text {i}"), base_ms + i as i64],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }

    // ---------------------------------------------------------------- stress ---

    #[test]
    fn stress_10k_text_500_image_full_pipeline() {
        let conn = test_db();
        let t0 = Instant::now();
        bulk_insert_text(&conn, 10_000, 1_700_000_000_000);
        let bulk = t0.elapsed();

        // dedup MISS path (one tx per upsert, as production does)
        let t = Instant::now();
        for i in 0..200 {
            upsert_text(&conn, &format!("fresh text {i}")).unwrap();
        }
        let upsert_miss_total = t.elapsed();

        // dedup HIT path (existing content, moves row to top)
        let t = Instant::now();
        for i in 0..200 {
            upsert_text(&conn, &format!("stress text {i}")).unwrap();
        }
        let upsert_hit_total = t.elapsed();

        // image upserts (dedup miss)
        let t = Instant::now();
        for i in 0..500 {
            upsert_image(&conn, &format!("C:\\img\\{i:016x}.png"), 800, 600).unwrap();
        }
        let img_total = t.elapsed();

        // full list fetch
        let t = Instant::now();
        let clips = list(&conn, "all", "").unwrap();
        let list_all = t.elapsed();
        assert_eq!(clips.len(), 500, "list is capped at 500 rows");
        for w in clips.windows(2) {
            assert!(
                (w[0].pinned, w[0].created_at) >= (w[1].pinned, w[1].created_at),
                "list not ordered by pinned DESC, created_at DESC"
            );
        }

        let t = Instant::now();
        let texts = list(&conn, "text", "").unwrap();
        let list_text = t.elapsed();
        assert!(texts.iter().all(|c| c.kind == "text" || c.kind == "html"));

        let t = Instant::now();
        let hits = list(&conn, "all", "stress text 123").unwrap();
        let list_search = t.elapsed();
        assert!(!hits.is_empty(), "search should find 'stress text 123'");

        let t = Instant::now();
        let st = stats(&conn).unwrap();
        let stats_t = t.elapsed();
        assert_eq!(st.total, 10_000 + 200 + 500);
        assert_eq!(st.image, 500);
        assert_eq!(st.text, 10_200);
        assert_eq!(st.paste_total, 10_000 + 400 + 500); // hit path bumped 200 rows by +1

        // enforce_limit pruning ~9500 rows (limit 1200)
        let t = Instant::now();
        let victims = enforce_limit(&conn, 1200).unwrap();
        let prune = t.elapsed();
        assert_eq!(victims.len(), 10_700 - 1200);
        assert_eq!(count(&conn), 1200);
        let oldest: i64 = conn
            .query_row("SELECT min(created_at) FROM clips", [], |r| r.get(0))
            .unwrap();
        let newest_unpinned_before_prune = 1_700_000_000_000 + 10_000 - 1;
        assert!(
            oldest > newest_unpinned_before_prune - 1200,
            "survivors should be the newest unpinned rows"
        );

        // prune at steady state (nothing to delete) — the common runtime path
        let t = Instant::now();
        let v2 = enforce_limit(&conn, 1200).unwrap();
        let prune_noop = t.elapsed();
        assert!(v2.is_empty());

        // pinned rows beyond the limit survive; pinned rows do NOT count toward
        // the limit (enforce_limit offsets only unpinned rows)
        let id_a: i64 = conn
            .query_row(
                "INSERT INTO clips(kind,preview,pinned,created_at) VALUES('text','pin-a',1,1) RETURNING id",
                [], |r| r.get(0)).unwrap();
        let id_b: i64 = conn
            .query_row(
                "INSERT INTO clips(kind,preview,pinned,created_at) VALUES('text','pin-b',1,2) RETURNING id",
                [], |r| r.get(0)).unwrap();
        let victims = enforce_limit(&conn, 1200).unwrap();
        assert!(
            victims.is_empty(),
            "at-limit unpinned count means no eviction"
        );
        // one new unpinned copy evicts exactly the oldest unpinned row
        upsert_text(&conn, "the tiebreaker").unwrap();
        let victims = enforce_limit(&conn, 1200).unwrap();
        assert_eq!(victims.len(), 1, "oldest unpinned row evicted");
        for id in [id_a, id_b] {
            let n: i64 = conn
                .query_row("SELECT count(*) FROM clips WHERE id=?1", [id], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "pinned row {id} must survive");
        }

        println!("\n=== STRESS TIMINGS (10k text + 200 upserts + 500 images) ===");
        println!(
            "bulk insert 10k rows (single tx):   {:>10.1?}  ({:.2} us/row)",
            bulk,
            bulk.as_micros() as f64 / 10_000.0
        );
        println!(
            "upsert dedup MISS x200:             {:>10.1?}  ({:.3} ms/op)",
            upsert_miss_total,
            upsert_miss_total.as_secs_f64() * 1000.0 / 200.0
        );
        println!(
            "upsert dedup HIT  x200:             {:>10.1?}  ({:.3} ms/op)",
            upsert_hit_total,
            upsert_hit_total.as_secs_f64() * 1000.0 / 200.0
        );
        println!(
            "image upsert x500:                  {:>10.1?}  ({:.3} ms/op)",
            img_total,
            img_total.as_secs_f64() * 1000.0 / 500.0
        );
        println!("list(all) full fetch (500 rows):    {:>10.1?}", list_all);
        println!("list(text) filtered:                {:>10.1?}", list_text);
        println!("list search LIKE 'stress text 123': {:>10.1?}", list_search);
        println!("stats (10.7k rows):                 {:>10.1?}", stats_t);
        println!("enforce_limit prune 9500 rows:      {:>10.1?}", prune);
        println!("enforce_limit no-op:                {:>10.1?}", prune_noop);
    }

    // ===================================================== deep stress suite ===
    // Run explicitly with: cargo test -- --ignored
    // These use a production-parity on-disk DB (WAL + ALL indexes, including the
    // partial dedup indexes), unlike test_db() which is in-memory and indexless.

    #[derive(Default, Clone, Copy)]
    struct CoreTimings {
        bulk: Duration,
        dedup_miss: Duration, // 200 full upsert_text calls (miss path, 1 tx each)
        dedup_hit: Duration,  // 200 full upsert_text calls (hit path)
        dedup_lookup: Duration, // 200 bare dedup SELECTs (no tx) — isolates index
        list_all: Duration,
        list_search: Duration,
        stats: Duration,
        prune: Duration,
    }

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "cvdeep_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Production-parity DB on disk: WAL + pinned index (+ optional dedup indexes).
    fn ondisk_db(dir: &std::path::Path, with_dedup_indexes: bool) -> Connection {
        let conn = Connection::open(dir.join("clips.db")).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clips(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               kind TEXT NOT NULL,
               content TEXT,
               image_path TEXT,
               html BLOB,
               files TEXT,
               preview TEXT NOT NULL,
               pinned INTEGER NOT NULL DEFAULT 0,
               use_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               text_path TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(pinned, created_at);",
        )
        .unwrap();
        if with_dedup_indexes {
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content) WHERE content IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_image_path ON clips(image_path) WHERE image_path IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_text_path ON clips(text_path) WHERE text_path IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_files ON clips(files) WHERE files IS NOT NULL;",
            )
            .unwrap();
        }
        conn
    }

    fn bulk_insert_text_disk(conn: &Connection, n: usize, base_ms: i64) {
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..n {
            conn.execute(
                "INSERT INTO clips(kind, content, preview, pinned, use_count, created_at) VALUES('text',?1,?1,0,1,?2)",
                rusqlite::params![format!("stress text {i}"), base_ms + i as i64],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }

    fn bulk_insert_images(conn: &Connection, n: usize, base_ms: i64) {
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..n {
            conn.execute(
                "INSERT INTO clips(kind, image_path, preview, pinned, use_count, created_at) VALUES('image',?1,?2,0,1,?3)",
                rusqlite::params![format!("C:\\img\\{i:016x}.png"), "[Image 800x600]".to_string(), base_ms + 500_000_000 + i as i64],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }

    /// Core op mix, mirroring stress_10k_text_500_image_full_pipeline.
    fn core_suite(conn: &Connection, n_text: usize, n_img: usize, limit: i64) -> CoreTimings {
        let mut t = CoreTimings::default();
        let t0 = Instant::now();
        bulk_insert_text_disk(conn, n_text, 1_700_000_000_000);
        bulk_insert_images(conn, n_img, 1_700_000_000_000);
        t.bulk = t0.elapsed();

        let t0 = Instant::now();
        for i in 0..200 {
            upsert_text(conn, &format!("fresh text {i}")).unwrap();
        }
        t.dedup_miss = t0.elapsed();

        let t0 = Instant::now();
        for i in 0..200 {
            upsert_text(conn, &format!("stress text {i}")).unwrap();
        }
        t.dedup_hit = t0.elapsed();

        // bare dedup lookup — no tx/commit noise, isolates the index itself
        let t0 = Instant::now();
        {
            let mut stmt = conn
                .prepare("SELECT id FROM clips WHERE content=?1 AND kind IN ('text','html')")
                .unwrap();
            for i in 0..200 {
                stmt.query_row(
                    [&format!("stress text {}", (i * 37) % n_text.max(1))],
                    |r| r.get::<_, i64>(0),
                )
                .ok();
            }
        }
        t.dedup_lookup = t0.elapsed();

        let t0 = Instant::now();
        let clips = list(conn, "all", "").unwrap();
        t.list_all = t0.elapsed();
        assert_eq!(clips.len(), 500);

        let t0 = Instant::now();
        let hits = list(conn, "all", "stress text 123").unwrap();
        t.list_search = t0.elapsed();
        assert!(!hits.is_empty());

        let t0 = Instant::now();
        let _ = stats(conn).unwrap();
        t.stats = t0.elapsed();

        let t0 = Instant::now();
        let victims = enforce_limit(conn, limit).unwrap();
        t.prune = t0.elapsed();
        // 200 fresh upserts add rows; the 200 dedup hits do not
        assert_eq!(
            victims.len() as i64,
            n_text as i64 + n_img as i64 + 200 - limit
        );
        t
    }

    fn ms(d: Duration) -> String {
        format!("{:>10.2} ms", d.as_secs_f64() * 1000.0)
    }

    fn per_op(d: Duration, n: u64) -> String {
        format!("{:>10.4} ms/op", d.as_secs_f64() * 1000.0 / n as f64)
    }

    #[test]
    #[ignore]
    fn deep_10k_vs_100k_scale_bench() {
        let dir = unique_dir("bench");
        let rows = [
            ("in-mem 10k", 10_000, false),
            ("on-disk 10k", 10_000, true),
            ("on-disk 100k", 100_000, true),
        ];
        let mut results: Vec<(String, CoreTimings)> = Vec::new();
        for (name, n, disk) in rows {
            let sub = dir.join(name.replace(' ', "_"));
            std::fs::create_dir_all(&sub).unwrap();
            let conn = if disk {
                ondisk_db(&sub, true)
            } else {
                test_db()
            };
            let limit = if n >= 100_000 { 5_000 } else { 1_200 };
            let t = core_suite(&conn, n, 500, limit);
            println!(
                "\n=== {name} ({} text + 500 img) done in {:.1?} ===",
                n,
                t.bulk
                    + t.dedup_miss
                    + t.dedup_hit
                    + t.list_all
                    + t.list_search
                    + t.stats
                    + t.prune
            );
            results.push((name.to_string(), t));
            drop(conn);
        }

        let mem10 = &results[0].1;
        let disk10 = &results[1].1;
        let disk100 = &results[2].1;

        println!("\n=== TIMING TABLE (10k vs 100k, on-disk WAL) ===");
        println!(
            "{:<34} {:>16} {:>16} {:>10}",
            "operation", "10k", "100k", "ratio"
        );
        let row = |label: &str, a: Duration, b: Duration| {
            println!(
                "{:<34} {:>16} {:>16} {:>9.2}x",
                label,
                ms(a),
                ms(b),
                b.as_secs_f64() / a.as_secs_f64().max(1e-9)
            );
        };
        row(
            "bulk insert per row (total)",
            disk10.bulk.div_f64(10_000.0),
            disk100.bulk.div_f64(100_000.0),
        );
        println!(
            "  dedup MISS per-op:  10k {}  100k {}",
            per_op(disk10.dedup_miss, 200),
            per_op(disk100.dedup_miss, 200)
        );
        println!(
            "  dedup HIT  per-op:  10k {}  100k {}",
            per_op(disk10.dedup_hit, 200),
            per_op(disk100.dedup_hit, 200)
        );
        println!(
            "  bare dedup lookup:  10k {}  100k {}",
            per_op(disk10.dedup_lookup, 200),
            per_op(disk100.dedup_lookup, 200)
        );
        println!("  in-mem dedup lookup: {}", per_op(mem10.dedup_lookup, 200));
        row("list 500 (all)", disk10.list_all, disk100.list_all);
        row("list search LIKE", disk10.list_search, disk100.list_search);
        row("stats", disk10.stats, disk100.stats);
        row("enforce_limit prune", disk10.prune, disk100.prune);

        // complexity verdict: with idx_clips_content the bare dedup lookup must be
        // near-flat. A linear scan would scale ~10x; O(log n) should stay < 3x.
        let ratio =
            disk100.dedup_lookup.as_secs_f64() / disk10.dedup_lookup.as_secs_f64().max(1e-9);
        assert!(
            ratio < 3.0,
            "dedup lookup scaled {ratio:.2}x from 10k->100k rows — looks linear, index not used?"
        );
        println!("\ndedup lookup 10k->100k scaling: {ratio:.2}x  => O(log n)/O(1) confirmed (<3x)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn deep_index_creation_migration_timing() {
        // simulates an existing user's first launch after the update: 100k-row DB
        // WITHOUT the dedup indexes, then CREATE INDEX runs
        let dir = unique_dir("migration");
        let conn = ondisk_db(&dir, false);
        bulk_insert_text_disk(&conn, 100_000, 1_700_000_000_000);
        bulk_insert_images(&conn, 5_000, 1_700_000_000_000);
        drop(conn);

        let conn = Connection::open(dir.join("clips.db")).unwrap();
        let idxs = [
            ("idx_clips_content",
             "CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content) WHERE content IS NOT NULL"),
            ("idx_clips_image_path",
             "CREATE INDEX IF NOT EXISTS idx_clips_image_path ON clips(image_path) WHERE image_path IS NOT NULL"),
            ("idx_clips_text_path",
             "CREATE INDEX IF NOT EXISTS idx_clips_text_path ON clips(text_path) WHERE text_path IS NOT NULL"),
            ("idx_clips_files",
             "CREATE INDEX IF NOT EXISTS idx_clips_files ON clips(files) WHERE files IS NOT NULL"),
        ];
        let t0 = Instant::now();
        for (name, sql) in idxs {
            let t = Instant::now();
            conn.execute_batch(&format!("{sql};")).unwrap();
            println!("CREATE INDEX {name:<22} {:>10.1?}", t.elapsed());
        }
        let total = t0.elapsed();
        println!("TOTAL migration index build (105k rows): {:.1?}", total);
        assert!(
            total.as_secs_f64() < 30.0,
            "migration took {total:?} — first launch would feel hung; consider doing it in a background thread or showing progress"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn deep_wal_growth_and_checkpoint() {
        let dir = unique_dir("wal");
        let conn = ondisk_db(&dir, true);
        bulk_insert_text_disk(&conn, 100_000, 1_700_000_000_000);
        bulk_insert_images(&conn, 5_000, 1_700_000_000_000);
        let victims = enforce_limit(&conn, 5_000).unwrap();
        assert_eq!(victims.len(), 100_000 + 5_000 - 5_000);

        let wal = dir.join("clips.db-wal");
        let wal_size = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
        let db_size = std::fs::metadata(dir.join("clips.db"))
            .map(|m| m.len())
            .unwrap_or(0);
        println!(
            "WAL size after 100k inserts + 95.5k deletes: {:.1} MiB (db: {:.1} MiB)",
            wal_size as f64 / 1048576.0,
            db_size as f64 / 1048576.0
        );

        let t = Instant::now();
        let checkpoint: i64 = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(1))
            .unwrap();
        let cp = t.elapsed();
        let wal_after = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
        println!(
            "wal_checkpoint(TRUNCATE): {:.1?} (busy={} frames), WAL after: {} bytes",
            cp, checkpoint, wal_after
        );
        assert_eq!(wal_after, 0, "TRUNCATE checkpoint must zero the WAL file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn deep_concurrency_shared_mutex_throughput_and_latency() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::{Arc, Mutex};

        let dir = unique_dir("conc");
        let conn = ondisk_db(&dir, true);
        bulk_insert_text_disk(&conn, 1_000, 1_700_000_000_000);
        let shared = Arc::new(Mutex::new(conn));
        let ops = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let mut handles = Vec::new();
        for thread_id in 0..4 {
            let shared = shared.clone();
            let ops = ops.clone();
            let stop = stop.clone();
            handles.push(std::thread::spawn(move || {
                let mut i: u64 = 0;
                while !stop.load(Ordering::Relaxed) {
                    let g = shared.lock().unwrap_or_else(|e| e.into_inner());
                    upsert_text(
                        &g,
                        &format!("thread {thread_id} iteration {i} unique payload"),
                    )
                    .unwrap();
                    let _ = list(&g, "all", "").unwrap();
                    if i % 20 == 0 {
                        let _ = enforce_limit(&g, 2_000).unwrap();
                    }
                    drop(g);
                    ops.fetch_add(3, Ordering::Relaxed);
                    i += 1;
                }
            }));
        }
        std::thread::sleep(Duration::from_secs(10));
        stop.store(true, Ordering::Relaxed);
        for h in handles {
            h.join().expect("worker thread must not panic");
        }
        let total = ops.load(Ordering::Relaxed);
        // a poisoned mutex would have surfaced as unwrap panic above; reaching
        // here means no deadlock (10s window passed) and no poisoning
        assert!(!shared.is_poisoned(), "mutex poisoned during stress");
        println!(
            "4 threads x 10s shared-mutex upsert+list+prune: {} ops => {:.0} ops/sec",
            total,
            total as f64 / 10.0
        );
        assert!(total > 100, "throughput suspiciously low: {total} ops/10s");

        // list p50/p99 while another thread inserts
        let inserting = {
            let shared = shared.clone();
            let stop = stop.clone();
            stop.store(false, Ordering::Relaxed);
            std::thread::spawn(move || {
                let mut i: u64 = 0;
                while !stop.load(Ordering::Relaxed) {
                    let g = shared.lock().unwrap();
                    bulk_insert_text_disk(&g, 50, 1_800_000_000_000 + (i * 50) as i64);
                    i += 1;
                }
            })
        };
        let mut latencies_us: Vec<u128> = Vec::new();
        for _ in 0..500 {
            let t = Instant::now();
            let g = shared.lock().unwrap();
            let clips = list(&g, "all", "").unwrap();
            drop(g);
            assert!(!clips.is_empty());
            latencies_us.push(t.elapsed().as_micros());
        }
        stop.store(true, Ordering::Relaxed);
        inserting.join().unwrap();
        latencies_us.sort();
        let p50 = latencies_us[latencies_us.len() / 2];
        let p99 = latencies_us[latencies_us.len() * 99 / 100];
        println!(
            "list latency under concurrent inserts: p50 {} us, p99 {} us, max {} us",
            p50,
            p99,
            latencies_us.last().unwrap()
        );
        // no hard bound: parallel bench tests can saturate the disk and skew
        // these numbers — this run reports, it does not gate
        if let Ok(g) = shared.lock() {
            let _ = g.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        }
        drop(shared);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_explain_dedup_queries_must_use_indexes() {
        // fast, non-ignored: production-parity schema, small data
        let dir = unique_dir("explain");
        let conn = ondisk_db(&dir, true);
        bulk_insert_text_disk(&conn, 1_000, 1_700_000_000_000);
        bulk_insert_images(&conn, 100, 1_700_000_000_000);
        conn.execute(
            "INSERT INTO clips(kind, text_path, preview, created_at) VALUES('text','C:\\t.txt','[T]',1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clips(kind, files, preview, created_at) VALUES('file','C:\\a.txt','[F]',1)",
            [],
        )
        .unwrap();
        let queries = [
            (
                "content",
                "SELECT id FROM clips WHERE content='x' AND kind IN ('text','html')",
                "idx_clips_content",
            ),
            (
                "image_path",
                "SELECT id FROM clips WHERE kind='image' AND image_path='x'",
                "idx_clips_image_path",
            ),
            (
                "text_path",
                "SELECT id FROM clips WHERE text_path='x'",
                "idx_clips_text_path",
            ),
            (
                "files",
                "SELECT id FROM clips WHERE kind='file' AND files='x'",
                "idx_clips_files",
            ),
        ];
        println!("\n=== EXPLAIN QUERY PLAN (dedup queries, production schema) ===");
        for (name, sql, expected_idx) in queries {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let detail: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(3))
                .unwrap()
                .flatten()
                .collect();
            println!("--- {name}: {}", detail.join(" | "));
            let joined = detail.join(" | ");
            assert!(
                joined.contains(expected_idx),
                "query '{name}' does not use {expected_idx}: {joined}"
            );
            assert!(
                !joined.to_uppercase().contains("SCAN"),
                "query '{name}' falls back to a table scan: {joined}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ------------------------------------------------------------- edge cases ---

    #[test]
    fn enforce_limit_zero_or_negative_is_noop() {
        let conn = test_db();
        bulk_insert_text(&conn, 10, 1);
        enforce_limit(&conn, 0).unwrap();
        enforce_limit(&conn, -5).unwrap();
        assert_eq!(count(&conn), 10, "max<=0 must not delete everything");
    }

    #[test]
    fn enforce_limit_exactly_at_limit_keeps_all() {
        let conn = test_db();
        bulk_insert_text(&conn, 100, 1);
        let victims = enforce_limit(&conn, 100).unwrap();
        assert!(victims.is_empty());
        assert_eq!(count(&conn), 100);
    }

    #[test]
    fn duplicate_upsert_moves_row_to_top_no_second_row() {
        let conn = test_db();
        upsert_text(&conn, "alpha").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        upsert_text(&conn, "beta").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let alpha_id = upsert_text(&conn, "alpha").unwrap().unwrap();
        assert_eq!(
            count(&conn),
            2,
            "duplicate upsert must not create a second row"
        );
        let clips = list(&conn, "all", "").unwrap();
        assert_eq!(clips[0].id, alpha_id, "re-copied row moves to top");
        assert!(clips[0].created_at >= clips[1].created_at);
        let uses: i64 = conn
            .query_row("SELECT use_count FROM clips WHERE id=?1", [alpha_id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(uses, 2);
    }

    #[test]
    fn delete_removes_side_files() {
        let dir = std::env::temp_dir().join(format!(
            "cvtest_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("abc.png");
        let thumb = dir.join("abc_t.png");
        let txt = dir.join("big.txt");
        std::fs::write(&img, b"png").unwrap();
        std::fs::write(&thumb, b"png").unwrap();
        std::fs::write(&txt, b"text").unwrap();

        let conn = test_db();
        conn.execute(
            "INSERT INTO clips(kind, preview, image_path, text_path, created_at) VALUES('image','[Image]',?1,?2,1)",
            rusqlite::params![img.to_string_lossy().as_ref(), txt.to_string_lossy().as_ref()],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM clips", [], |r| r.get(0))
            .unwrap();

        delete(&conn, id).unwrap();
        assert_eq!(count(&conn), 0, "row deleted");
        assert!(!img.exists(), "image file removed");
        assert!(!thumb.exists(), "thumbnail file removed");
        assert!(!txt.exists(), "oversized-text side file removed");
        // delete of a missing id is fine
        delete(&conn, 999).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_all_empties_and_returns_files() {
        let conn = test_db();
        bulk_insert_text(&conn, 50, 1);
        upsert_image(&conn, "C:\\x.png", 1, 1).unwrap();
        let victims = clear_all(&conn).unwrap();
        assert_eq!(count(&conn), 0);
        assert_eq!(victims.len(), 51);
        assert_eq!(victims.iter().filter(|(i, _)| i.is_some()).count(), 1);
        assert_eq!(stats(&conn).unwrap().total, 0);
    }

    #[test]
    fn toggle_pin_flips_and_stats_counts() {
        let conn = test_db();
        let a = upsert_text(&conn, "http://example.com").unwrap().unwrap();
        upsert_text(&conn, "https://rust-lang.org").unwrap();
        upsert_text(&conn, "plain note").unwrap();
        upsert_image(&conn, "C:\\i.png", 1, 1).unwrap();
        upsert_file(&conn, &["C:\\f.txt".into()]).unwrap();
        toggle_pin(&conn, a).unwrap();
        let st = stats(&conn).unwrap();
        assert_eq!(st.total, 5);
        assert_eq!(st.text, 3);
        assert_eq!(st.image, 1);
        assert_eq!(st.link, 2);
        assert_eq!(st.file, 1);
        assert_eq!(st.pinned, 1);
        assert_eq!(st.today, 5);
        assert_eq!(st.paste_total, 5);
        assert_eq!(st.daily.len(), 7);
        assert_eq!(st.top.len(), 5);
        let clips = list(&conn, "all", "").unwrap();
        assert!(clips[0].pinned);
    }

    #[test]
    fn upsert_text_empty_and_whitespace_only_ignored() {
        let conn = test_db();
        assert_eq!(upsert_text(&conn, ""), Ok(None));
        assert_eq!(upsert_text(&conn, "   \n\t "), Ok(None));
        assert_eq!(count(&conn), 0);
    }

    #[test]
    fn upsert_file_empty_paths_ignored_and_roundtrip() {
        let conn = test_db();
        assert_eq!(upsert_file(&conn, &[]), Ok(None));
        let id = upsert_file(&conn, &["C:\\a.txt".into(), "C:\\b.txt".into()])
            .unwrap()
            .unwrap();
        assert_eq!(
            get_files(&conn, id).unwrap(),
            vec!["C:\\a.txt".to_string(), "C:\\b.txt".to_string()]
        );
    }

    // ------------------------------------------------------- query plan audit ---

    #[test]
    fn explain_query_plans() {
        let conn = test_db();
        bulk_insert_text(&conn, 100, 1);
        let plans = [
            ("list_clips no-search",
             "SELECT id FROM clips WHERE 1=1 ORDER BY pinned DESC, created_at DESC LIMIT 500"),
            ("list_clips search",
             "SELECT * FROM (SELECT id, pinned, created_at FROM clips WHERE pinned=1 AND preview LIKE '%x%' ESCAPE '\\' UNION ALL SELECT id, pinned, created_at FROM clips WHERE pinned=0 AND preview LIKE '%x%' ESCAPE '\\') ORDER BY pinned DESC, created_at DESC LIMIT 500"),
            ("enforce_limit OFFSET",
             "SELECT id FROM clips WHERE pinned=0 ORDER BY created_at DESC LIMIT -1 OFFSET 5"),
            ("dedup text",
             "SELECT id FROM clips WHERE content='x' AND kind IN ('text','html')"),
            ("dedup image",
             "SELECT id FROM clips WHERE kind='image' AND image_path='x'"),
            ("dedup file",
             "SELECT id FROM clips WHERE kind='file' AND files='x'"),
            ("dedup text_file",
             "SELECT id FROM clips WHERE text_path='x'"),
            ("stats link count",
             "SELECT count(*) FROM clips WHERE kind='text' AND (content LIKE 'http://%' OR content LIKE 'https://%')"),
            ("stats top-5",
             "SELECT preview FROM clips ORDER BY use_count DESC, created_at DESC LIMIT 5"),
        ];
        println!("\n=== EXPLAIN QUERY PLAN ===");
        for (name, sql) in plans {
            println!("--- {name}: {sql}");
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = stmt
                .query_map([], |r| Ok(format!("  {}", r.get::<_, String>(3)?)))
                .unwrap();
            for r in rows.flatten() {
                println!("{r}");
            }
        }
    }
}

#[cfg(test)]
mod dedup_tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE clips(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               kind TEXT NOT NULL, content TEXT, image_path TEXT, html BLOB,
               files TEXT, preview TEXT NOT NULL,
               pinned INTEGER NOT NULL DEFAULT 0,
               use_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn same_content_stays_one_row_across_kinds() {
        let conn = mem();
        upsert_text(&conn, "hello world").unwrap();
        upsert_html(&conn, "hello world", b"<b>hello world</b>").unwrap(); // plain dup upgrades to html
        upsert_text(&conn, "hello world").unwrap(); // plain copy of the html entry bumps it
        let n: i64 = conn
            .query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "same content must stay a single row");
        let kind: String = conn
            .query_row("SELECT kind FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind, "html");
    }
}
