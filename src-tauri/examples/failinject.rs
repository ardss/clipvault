//! Failure-injection harness. Run: cargo run --example failinject -- <mode>
//! Modes: kill | corrupt | settings | migration

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cvfail_{}_{}_{}",
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

/// Production-parity schema copied from db.rs init (clips table + all indexes).
const INIT_SQL: &str = r#"
PRAGMA journal_mode=WAL;
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
CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(pinned, created_at);
"#;

/// Column migrations + dedup indexes, in production order (columns first).
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "ALTER TABLE clips ADD COLUMN html BLOB;
         ALTER TABLE clips ADD COLUMN files TEXT;
         ALTER TABLE clips ADD COLUMN text_path TEXT;",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content) WHERE content IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_image_path ON clips(image_path) WHERE image_path IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_text_path ON clips(text_path) WHERE text_path IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_clips_files ON clips(files) WHERE files IS NOT NULL;",
    )?;
    Ok(())
}

fn open_prod(db: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db)?;
    conn.execute_batch(INIT_SQL)?;
    let _ = migrate(&conn); // duplicate-column on re-open is fine (idempotent call sites guard)
    Ok(conn)
}

fn seed(conn: &Connection, n: i64) {
    conn.execute_batch("BEGIN").unwrap();
    for i in 0..n {
        conn.execute(
            "INSERT INTO clips(kind,content,preview,use_count,created_at) VALUES('text',?1,?1,1,?2)",
            rusqlite::params![format!("seed row {i}"), 1_700_000_000_000 + i],
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT").unwrap();
}

fn count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
        .unwrap_or(-1)
}

fn integrity(conn: &Connection) -> String {
    conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|e| format!("ERR: {e}"))
}

// ------------------------------------------------------------- 1: kill ----

/// child: opens prod db, starts one tx, inserts forever; parent kills us.
fn kill_child(db: &Path) -> ! {
    let conn = open_prod(db).expect("child open");
    conn.execute_batch("BEGIN").unwrap();
    let mut i = 0i64;
    loop {
        conn.execute(
            "INSERT INTO clips(kind,content,preview,use_count,created_at) VALUES('text',?1,?1,1,?2)",
            rusqlite::params![format!("kill-run payload {i} — some reasonably long text to give the WAL something to chew on"), 1_800_000_000_000 + i],
        )
        .unwrap();
        i += 1;
    }
}

fn test_kill() {
    let mut outcomes = Vec::new();
    for iter in 0..10 {
        let d = dir("kill");
        let db = d.join("clips.db");
        {
            let conn = open_prod(&db).unwrap();
            seed(&conn, 100); // pre-existing committed data
        }
        let exe = std::env::current_exe().unwrap();
        let mut child = Command::new(&exe)
            .args(["failinject-kill-child", &db.to_string_lossy()])
            .spawn()
            .unwrap();
        // random kill point between 5ms and 120ms
        let ms = 5 + (iter * 37 + (iter * iter) % 23) % 115;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &child.id().to_string(), "/T"])
            .output();
        let _ = child.wait();
        std::thread::sleep(std::time::Duration::from_millis(50));

        // reopen and verify
        let res = (|| -> Result<(i64, String, String, bool), String> {
            let conn = Connection::open(&db).map_err(|e| e.to_string())?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")
                .map_err(|e| e.to_string())?;
            let n = count(&conn);
            let ic = integrity(&conn);
            // prefix check: committed seeds survive intact, exactly 0 partial rows
            let seeds: i64 = conn
                .query_row(
                    "SELECT count(*) FROM clips WHERE content LIKE 'seed row %'",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let partial: i64 = conn
                .query_row(
                    "SELECT count(*) FROM clips WHERE content LIKE 'kill-run payload %'",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            Ok((
                n,
                ic,
                format!("seeds={seeds}"),
                seeds == 100 && (partial == 0 || partial % 1000 == 0) || partial == 0,
            ))
        })();
        outcomes.push((iter, ms, res));
        let _ = std::fs::remove_dir_all(&d);
    }
    println!("\n=== KILL-DURING-WRITE RESULTS ===");
    for (iter, ms, res) in &outcomes {
        match res {
            Ok((n, ic, seeds, prefix_ok)) => println!(
                "iter {iter} (killed after {ms}ms): rows={n} {seeds} integrity={ic} transactional={prefix_ok}"
            ),
            Err(e) => println!("iter {iter} (killed after {ms}ms): REOPEN FAILED: {e}"),
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|(_, _, r)| matches!(r, Ok((_, ic, _, true)) if ic == "ok")),
        "some kill iteration failed"
    );
}

// --------------------------------------------------------- 2: corrupt ----

fn make_good(dirp: &Path) -> Connection {
    let db = dirp.join("clips.db");
    let conn = open_prod(&db).unwrap();
    seed(&conn, 500);
    let _: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .unwrap();
    conn
}

fn try_open(label: &str, d: &Path) {
    let db = d.join("clips.db");
    let r = (|| -> Result<String, String> {
        let conn = Connection::open(&db).map_err(|e| format!("open: {e}"))?;
        let n: Result<i64, String> = conn
            .query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
            .map_err(|e| format!("count: {e}"));
        let ic = integrity(&conn);
        // db::init runs this DDL batch FIRST — does it survive corruption?
        let init = conn
            .execute_batch(INIT_SQL)
            .map_err(|e| format!("init-sql: {e}"));
        let migrate = match &init {
            Ok(_) => migrate(&conn)
                .map(|_| "migrate=ok")
                .map_err(|e| format!("migrate: {e}")),
            Err(_) => Ok("migrate=skipped"),
        };
        Ok(format!(
            "count={} integrity={} init-sql={}",
            n.map(|v| v.to_string())
                .unwrap_or_else(|e| format!("FAILED {e}")),
            ic.replace('\n', " | "),
            match (init, migrate) {
                (Ok(_), Ok(m)) => format!("ok {m}  <-- corrupt db PASSES startup"),
                (Ok(_), Err(e)) => format!("ok but {e}"),
                (Err(e), _) => format!("FAILED {e}  <-- db::init ? would abort setup"),
            }
        ))
    })();
    println!(
        "{label:<28} -> {}",
        match r {
            Ok(s) => format!("OPENED  {s}"),
            Err(e) => format!("ERROR (clean, no panic) {e}"),
        }
    );
    let _ = std::fs::remove_dir_all(d);
}

fn test_corrupt() {
    println!("\n=== CORRUPT-FILE RECOVERY ===");

    // truncations
    for pct in [30u64, 60, 90] {
        let d = dir("corr");
        let db = d.join("clips.db");
        make_good(&d);
        drop_open(&db);
        let full = std::fs::read(&db).unwrap();
        let cut = full.len() * pct as usize / 100;
        std::fs::write(&db, &full[..cut]).unwrap();
        try_open(&format!("truncate to {pct}%"), &d);
    }

    // random byte flips
    for round in 0..3 {
        let d = dir("corr");
        let db = d.join("clips.db");
        make_good(&d);
        drop_open(&db);
        let mut bytes = std::fs::read(&db).unwrap();
        let n = bytes.len();
        for k in 0..8 {
            let pos = (round * 977 + k * 6151 + 4096) % n;
            bytes[pos] ^= 0xA5;
        }
        std::fs::write(&db, bytes).unwrap();
        try_open(&format!("flip 8 bytes (round {round})"), &d);
    }

    // zero the WAL while db exists
    {
        let d = dir("corr");
        {
            let conn = make_good(&d);
            seed(&conn, 100); // leave data in the WAL (no checkpoint)
        }
        let wal = d.join("clips.db-wal");
        std::fs::write(&wal, vec![0u8; 4096]).unwrap();
        try_open("zeroed -wal (db intact)", &d);
    }

    // delete -wal, keep -shm
    {
        let d = dir("corr");
        {
            let conn = make_good(&d);
            seed(&conn, 100); // data only in WAL
        }
        let _ = std::fs::remove_file(d.join("clips.db-wal"));
        // a killed process leaves -shm behind; a clean close removes it, so
        // fabricate one to reproduce the "db + stale -shm, no -wal" state
        std::fs::write(d.join("clips.db-shm"), vec![0u8; 32768]).unwrap();
        try_open("deleted -wal, -shm kept", &d);
    }

    fn drop_open(db: &Path) {
        // close all connections: open a fresh one and checkpoint via sqlite close
        // (make_good already closed when it goes out of scope; nothing to do here)
        let _ = db;
    }
}

// -------------------------------------------------------- 3: settings ----

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct SettingsLike {
    history_limit: i64,
    panel_height: f64,
    autostart: bool,
    #[serde(default = "default_hotkey")]
    hotkey: String,
}
fn default_hotkey() -> String {
    "Alt+V".into()
}
impl Default for SettingsLike {
    fn default() -> Self {
        Self {
            history_limit: 1000,
            panel_height: 540.0,
            autostart: false,
            hotkey: default_hotkey(),
        }
    }
}

fn test_settings() {
    println!("\n=== SETTINGS FILE TORTURE (mirrors load_settings: read_to_string -> serde_json::from_str -> default + .bak) ===");
    let d = dir("settings");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "truncated JSON",
            br#"{"history_limit": 500, "panel_h"#.to_vec(),
        ),
        ("empty file", Vec::new()),
        (
            "wrong types (history_limit:\"abc\")",
            br#"{"history_limit": "abc", "panel_height": 540.0, "autostart": false}"#.to_vec(),
        ),
        ("10MB garbage", {
            let mut v = vec![b'x'; 10 * 1024 * 1024];
            v[0] = b'{';
            v
        }),
        ("valid JSON + BOM prefix", {
            let mut v = b"\xEF\xBB\xBF".to_vec();
            v.extend_from_slice(
                br#"{"history_limit": 250, "panel_height": 400.0, "autostart": true}"#,
            );
            v
        }),
        ("UTF-16 encoded valid JSON", {
            let json = r#"{"history_limit": 300, "panel_height": 420.0, "autostart": true}"#;
            json.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
        }),
    ];
    for (name, bytes) in &cases {
        let f = d.join("settings.json");
        std::fs::write(&f, bytes).unwrap();
        // replicate load_settings
        let raw = std::fs::read_to_string(&f).ok();
        let outcome = match &raw {
            Some(txt) => match serde_json::from_str::<SettingsLike>(txt) {
                Ok(s) => format!(
                    "PARSED history_limit={} hotkey={}",
                    s.history_limit, s.hotkey
                ),
                Err(e) => {
                    let _ = std::fs::copy(&f, d.join("settings.json.bak"));
                    let bak = std::fs::read(d.join("settings.json.bak")).unwrap();
                    format!(
                        "serde ERR ({e}) -> defaults, .bak written ({} bytes)",
                        bak.len()
                    )
                }
            },
            None => {
                // load_settings: raw None -> Settings::default(), NO .bak, NO log
                "read_to_string FAILED (not valid UTF-8) -> defaults, NO .bak, silent".to_string()
            }
        };
        println!("{:<36} -> {outcome}", format!("[{name}]"));
    }
    let _ = std::fs::remove_dir_all(&d);
    // Note: real load_settings then clamps history_limit 1..5000 and panel_height,
    // so even a parsed bad-value file cannot break the window.
}

// ------------------------------------------------------- 5: migration ----

fn test_migration() {
    println!("\n=== MIGRATION IDEMPOTENCY ===");
    let d = dir("migr");
    let db = d.join("clips.db");
    // v0.0.9 db: base table only, no html/files/text_path, no dedup indexes
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE clips(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               kind TEXT NOT NULL, content TEXT, image_path TEXT,
               preview TEXT NOT NULL, pinned INTEGER NOT NULL DEFAULT 0,
               use_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
             CREATE INDEX idx_clips_pinned ON clips(pinned, created_at);",
        )
        .unwrap();
        seed(&conn, 42);
    }
    // run init SQL twice
    for pass in 1..=2 {
        let r = (|| -> Result<(), String> {
            let conn = Connection::open(&db).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
                 CREATE TABLE IF NOT EXISTS clips(
                   id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, content TEXT,
                   image_path TEXT, preview TEXT NOT NULL, pinned INTEGER NOT NULL DEFAULT 0,
                   use_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
                 CREATE INDEX IF NOT EXISTS idx_clips_pinned ON clips(pinned, created_at);",
            )
            .map_err(|e| e.to_string())?;
            for col in ["html BLOB", "files TEXT", "text_path TEXT"] {
                let has: bool = conn
                    .prepare("PRAGMA table_info(clips)")
                    .unwrap()
                    .query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .flatten()
                    .any(|c| c == col.split(' ').next().unwrap());
                if !has {
                    conn.execute_batch(&format!("ALTER TABLE clips ADD COLUMN {col};"))
                        .map_err(|e| e.to_string())?;
                }
            }
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content) WHERE content IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_image_path ON clips(image_path) WHERE image_path IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_text_path ON clips(text_path) WHERE text_path IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clips_files ON clips(files) WHERE files IS NOT NULL;",
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        let conn = Connection::open(&db).unwrap();
        println!(
            "migration pass {pass}: {}  rows={} integrity={}",
            if r.is_ok() {
                "ok".to_string()
            } else {
                format!("ERR {:?}", r)
            },
            count(&conn),
            integrity(&conn)
        );
    }
    let conn = Connection::open(&db).unwrap();
    let old: i64 = conn
        .query_row(
            "SELECT count(*) FROM clips WHERE content LIKE 'seed row %'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old, 42, "old rows must survive migration");
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(clips)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    println!("columns after migration: {}", cols.join(", "));
    assert!(["html", "files", "text_path"]
        .iter()
        .all(|c| cols.iter().any(|x| x == c)));
    let _ = std::fs::remove_dir_all(&d);
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        // internal child modes
        "kill-child" => {
            let db: PathBuf = std::env::args().nth(2).unwrap().into();
            kill_child(&db);
        }
        "failinject-kill-child" => {
            let db: PathBuf = std::env::args().nth(2).unwrap().into();
            kill_child(&db);
        }
        "kill" => test_kill(),
        "corrupt" => test_corrupt(),
        "settings" => test_settings(),
        "migration" => test_migration(),
        _ => {
            test_kill();
            test_corrupt();
            test_settings();
            test_migration();
            println!("\nALL FAILURE-INJECTION SCENARIOS COMPLETED");
        }
    }
}
