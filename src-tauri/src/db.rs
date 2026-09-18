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
    let conn = Connection::open(dir.join("clips.db")).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
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
    app.manage(Db(Mutex::new(conn)));
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

const COLS: &str = "id, kind, preview, image_path, content, pinned, use_count, created_at, text_path";

pub fn list(conn: &Connection, filter: &str, query: &str) -> Result<Vec<Clip>, String> {
    cvlog!("[cv] list: filter={filter:?} query={query:?} total={}", conn.query_row("SELECT count(*) FROM clips", [], |r| r.get::<_,i64>(0)).unwrap_or(-1));
    let base = match filter {
        "text" => " AND kind IN ('text','html')",
        "image" => " AND kind='image'",
        "file" => " AND kind='file'",
        "link" => " AND kind='text' AND (content LIKE 'http://%' OR content LIKE 'https://%')",
        _ => "",
    };
    // escape LIKE wildcards in user input, bind as a parameter
    let like = format!(
        "%{}%",
        query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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
    .map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();
    Ok(rows)
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

pub fn enforce_limit(conn: &Connection, max: i64) -> Result<Vec<(Option<String>, Option<String>)>, String> {
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
            .query_map([max], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
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


pub fn upsert_text_file(conn: &Connection, preview: &str, path: &str) -> Result<Option<i64>, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let dup: Option<i64> = tx
        .query_row(
            "SELECT id FROM clips WHERE text_path=?1",
            [path],
            |r| r.get::<_, i64>(0),
        )
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

/// Full text of an oversized entry (lives in the side file).
#[allow(dead_code)]
pub fn get_text_file(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    let p: Option<String> = conn
        .query_row("SELECT text_path FROM clips WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(p.and_then(|p| std::fs::read_to_string(p).ok()))
}

pub fn toggle_pin(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE clips SET pinned = 1 - pinned WHERE id=?1",
        [id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes a clip row and its on-disk files: the PNG, its `_t.png` thumbnail
/// and, for oversized text entries, the full-text side file.
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let (image_path, text_path): (Option<String>, Option<String>) = tx
        .query_row(
            "SELECT image_path, text_path FROM clips WHERE id=?1",
            [id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
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
            let thumb = std::path::Path::new(p)
                .with_file_name(format!("{}_t.png", stem.to_string_lossy()));
            let _ = std::fs::remove_file(thumb);
        }
    }
    if let Some(p) = text_path {
        let _ = std::fs::remove_file(p);
    }
}

pub fn get_clip(conn: &Connection, id: i64) -> Result<(String, Option<String>, Option<String>), String> {
    conn.query_row(
        "SELECT kind, content, image_path FROM clips WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .map_err(|e| e.to_string())
}

pub fn mark_used(conn: &Connection, id: i64) {
    let _ = conn.execute(
        "UPDATE clips SET use_count=use_count+1, created_at=?2 WHERE id=?1",
        rusqlite::params![id, now()],
    );
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    let one = |sql: &str| -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
    };
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
        .prepare("SELECT preview, use_count FROM clips ORDER BY use_count DESC, created_at DESC LIMIT 5")
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
        link: one("SELECT count(*) FROM clips WHERE kind='text' AND (content LIKE 'http://%' OR content LIKE 'https://%')"),
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
            p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| paths[0].clone())
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
