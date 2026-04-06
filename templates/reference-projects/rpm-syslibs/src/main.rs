use openssl::version;
use rusqlite::Connection;

fn main() {
    // Verify system OpenSSL is linked and functional
    println!("openssl: {}", version::version());

    // Verify system SQLite is linked and functional
    let db = Connection::open_in_memory().expect("failed to open in-memory database");
    db.execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT)")
        .expect("failed to create table");
    db.execute("INSERT INTO t(val) VALUES(?1)", ["hello from syslibs"])
        .expect("failed to insert row");
    let val: String = db
        .query_row("SELECT val FROM t WHERE id = 1", [], |row| row.get(0))
        .expect("failed to query row");
    println!("sqlite: version={}, roundtrip={val}", rusqlite::version());
}
