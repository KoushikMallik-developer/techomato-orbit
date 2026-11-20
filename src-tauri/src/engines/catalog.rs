//! The engines Orbit can create instances of. Every download is pinned to a URL and a SHA-256
//! taken from the exact file that was tested; a file that does not match is never unpacked.

use crate::models::service::Category;
use std::path::Path;

pub struct Engine {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub default_port: u16,
    pub category: Category,
    /// The server program, relative to the unpacked folder.
    pub server: &'static str,
    /// The program that prepares an empty data folder, when the engine needs one.
    pub init: Option<&'static str>,
}

pub const ENGINES: &[Engine] = &[
    Engine {
        id: "postgres",
        name: "PostgreSQL",
        version: "16.4",
        url: "https://get.enterprisedb.com/postgresql/postgresql-16.4-1-windows-x64-binaries.zip",
        sha256: "3508d8f085bc3980f38211a82e3f31e5fcae9952105d3dc2f8be67b64a822baa",
        bytes: 338_727_828,
        default_port: 5432,
        category: Category::Database,
        server: "pgsql/bin/postgres.exe",
        init: Some("pgsql/bin/initdb.exe"),
    },
    Engine {
        id: "mariadb",
        name: "MariaDB (MySQL-compatible)",
        version: "11.4.3",
        url: "https://archive.mariadb.org/mariadb-11.4.3/winx64-packages/mariadb-11.4.3-winx64.zip",
        sha256: "208e9437fd769821d2d11f294d33d89f7a632f6a6d17177274da380397bd100b",
        bytes: 90_566_436,
        default_port: 3306,
        category: Category::Database,
        server: "mariadb-11.4.3-winx64/bin/mysqld.exe",
        init: Some("mariadb-11.4.3-winx64/bin/mariadb-install-db.exe"),
    },
    // Unofficial community port (there is no official Redis for Windows); version 5 from 2022.
    Engine {
        id: "redis",
        name: "Redis",
        version: "5.0.14.1",
        url: "https://github.com/tporadowski/redis/releases/download/v5.0.14.1/Redis-x64-5.0.14.1.zip",
        sha256: "018ea18a35876383cbb5f4cd0258adfc87747cf9d619bce1cf73a2e36f720ccf",
        bytes: 12_617_669,
        default_port: 6379,
        category: Category::Cache,
        server: "redis-server.exe",
        init: None,
    },
    Engine {
        id: "mongo",
        name: "MongoDB",
        version: "7.0.14",
        url: "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-7.0.14.zip",
        sha256: "59aa714af531e2b68ce01fa1be9999435fb9bc2b2002439f3271470113bdf7fe",
        bytes: 620_364_514,
        default_port: 27017,
        category: Category::Database,
        server: "mongodb-win32-x86_64-windows-7.0.14/bin/mongod.exe",
        init: None,
    },
];

pub fn find(id: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|e| e.id == id)
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Arguments for the init program (`Engine::init`), run once on a new data folder.
pub fn init_args(e: &Engine, data: &Path) -> Vec<String> {
    match e.id {
        "postgres" => [
            "-D",
            &s(data),
            "-U",
            "postgres",
            "-A",
            "trust",
            "-E",
            "UTF8",
        ]
        .map(String::from)
        .to_vec(),
        "mariadb" => vec![format!("--datadir={}", s(data))],
        _ => vec![],
    }
}

/// Arguments that run the server on `port`, bound to this PC only.
pub fn run_args(e: &Engine, data: &Path, port: u16) -> Vec<String> {
    let p = port.to_string();
    match e.id {
        "postgres" => vec![
            "-D".into(),
            s(data),
            "-p".into(),
            p,
            "-c".into(),
            "listen_addresses=127.0.0.1".into(),
        ],
        "mariadb" => vec![
            format!("--datadir={}", s(data)),
            format!("--port={p}"),
            "--bind-address=127.0.0.1".into(),
            "--console".into(),
        ],
        "redis" => vec![
            "--port".into(),
            p,
            "--bind".into(),
            "127.0.0.1".into(),
            "--dir".into(),
            s(data),
        ],
        "mongo" => vec![
            "--dbpath".into(),
            s(data),
            "--port".into(),
            p,
            "--bind_ip".into(),
            "127.0.0.1".into(),
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_is_pinned_and_unique() {
        let mut ids: Vec<_> = ENGINES.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ENGINES.len());
        for e in ENGINES {
            assert!(e.url.starts_with("https://"), "{}", e.id);
            assert_eq!(e.sha256.len(), 64, "{}", e.id);
            assert!(e.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", e.id);
            assert!(e.bytes > 0, "{}", e.id);
        }
    }

    #[test]
    fn every_engine_runs_on_its_port_in_its_data_folder_on_this_pc_only() {
        let data = Path::new("D:/data");
        for e in ENGINES {
            let args = run_args(e, data, 4242).join(" ");
            assert!(args.contains("4242"), "{} port", e.id);
            assert!(args.contains("D:/data"), "{} data", e.id);
            assert!(args.contains("127.0.0.1"), "{} bind", e.id);
        }
    }
}
