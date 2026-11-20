//! Download, verify and unpack an engine under `<data>/engines/<id>-<version>/`.

use super::catalog::Engine;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn engine_dir(root: &Path, e: &Engine) -> PathBuf {
    root.join("engines").join(format!("{}-{}", e.id, e.version))
}

pub fn server_path(root: &Path, e: &Engine) -> PathBuf {
    engine_dir(root, e).join(e.server)
}

pub fn is_installed(root: &Path, e: &Engine) -> bool {
    server_path(root, e).is_file()
}

/// The error a cancelled download returns; callers compare against it to tell it from a failure.
pub const CANCELLED: &str = "The download was cancelled.";

/// Streams `url` into `dest`, returning the SHA-256 of what was written. `progress(done, total)`
/// is called as bytes arrive (`total` is 0 when the server does not say). Setting `cancel` stops it.
pub fn download(
    url: &str,
    dest: &Path,
    progress: &dyn Fn(u64, u64),
    cancel: &AtomicBool,
) -> Result<String, String> {
    // reqwest is built without a bundled TLS provider; install one if nobody has yet.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Could not download: {e}"))?;
    let total = resp.content_length().unwrap_or(0);
    let mut out = File::create(dest).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, 0u64);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let n = resp
            .read(&mut buf)
            .map_err(|e| format!("The download was interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if done - last >= 2 * 1024 * 1024 {
            last = done;
            progress(done, total);
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    progress(done, total);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Checks the hash, then unpacks. A file that does not match is deleted and nothing is unpacked.
/// Unpacks into a temporary folder first, so a half-unpacked engine is never mistaken for a
/// finished one. `zip` refuses entries that would land outside the folder.
pub fn finish(root: &Path, e: &Engine, archive: &Path, actual_sha256: &str) -> Result<(), String> {
    if !actual_sha256.eq_ignore_ascii_case(e.sha256) {
        let _ = fs::remove_file(archive);
        return Err(format!(
            "The {} download did not match the expected checksum, so it was not installed.",
            e.name
        ));
    }
    let dir = engine_dir(root, e);
    let tmp = dir.with_extension("tmp");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|x| x.to_string())?;
    let unpacked = File::open(archive)
        .map_err(|x| x.to_string())
        .and_then(|f| zip::ZipArchive::new(f).map_err(|x| x.to_string()))
        .and_then(|mut z| z.extract(&tmp).map_err(|x| x.to_string()));
    let _ = fs::remove_file(archive);
    if let Err(x) = unpacked {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("Could not unpack {}: {x}", e.name));
    }
    if !tmp.join(e.server).is_file() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("{} did not contain {}.", e.name, e.server));
    }
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&tmp, &dir).map_err(|x| x.to_string())
}

/// Free bytes on the drive that holds `path`, or None when that can't be told.
fn free_space(path: &Path) -> Option<u64> {
    let want = path.to_string_lossy().to_lowercase();
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|d| want.starts_with(&d.mount_point().to_string_lossy().to_lowercase()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// The archive plus the unpacked files need about four times the download size.
fn too_little_space(e: &Engine, free: u64) -> Option<String> {
    let need = e.bytes.saturating_mul(4);
    let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
    (free < need).then(|| {
        format!(
            "{} needs about {:.1} GB free on this drive, and only {:.1} GB is free.",
            e.name,
            gb(need),
            gb(free)
        )
    })
}

/// Makes sure the engine is installed, downloading it when it is not.
pub fn ensure(
    root: &Path,
    e: &Engine,
    progress: &dyn Fn(&str, u64, u64),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if is_installed(root, e) {
        return Ok(server_path(root, e));
    }
    let dir = root.join("engines");
    fs::create_dir_all(&dir).map_err(|x| x.to_string())?;
    if let Some(m) = free_space(&dir).and_then(|free| too_little_space(e, free)) {
        return Err(m);
    }
    let archive = dir.join(format!("{}-{}.zip.part", e.id, e.version));
    progress("download", 0, 0);
    let hash = download(e.url, &archive, &|d, t| progress("download", d, t), cancel).inspect_err(
        |_| {
            let _ = fs::remove_file(&archive);
        },
    )?;
    progress("unpack", 0, 0);
    finish(root, e, &archive, &hash)?;
    Ok(server_path(root, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::service::Category;
    use std::io::Write;

    fn engine(sha: &str) -> Engine {
        Engine {
            id: "t",
            name: "Test",
            version: "1",
            url: "https://example.invalid/t.zip",
            sha256: Box::leak(sha.to_string().into_boxed_str()),
            bytes: 0,
            default_port: 1,
            category: Category::Database,
            server: "bin/server.exe",
            init: None,
        }
    }

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) -> String {
        let mut w = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, body) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body).unwrap();
        }
        w.finish().unwrap();
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
    }

    /// Serves `body` once over HTTP on a free local port and returns its URL.
    fn serve(body: &'static [u8]) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/f.zip", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let _ = s.read(&mut [0u8; 1024]);
                let head = format!(
                    "HTTP/1.1 200 OK
Content-Length: {}

",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(body);
            }
        });
        url
    }

    #[test]
    fn a_download_returns_the_hash_of_what_arrived() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.zip");
        let url = serve(b"hello orbit");
        let sha = download(&url, &dest, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(sha, format!("{:x}", Sha256::digest(b"hello orbit")));
        assert_eq!(fs::read(&dest).unwrap(), b"hello orbit");
    }

    #[test]
    fn a_cancelled_download_stops_with_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let url = serve(b"hello orbit");
        let err = download(
            &url,
            &dir.path().join("f.zip"),
            &|_, _| {},
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(err, CANCELLED);
    }

    #[test]
    fn too_little_free_space_is_refused_before_downloading() {
        let mut e = engine(&"0".repeat(64));
        e.bytes = 1 << 30;
        assert!(too_little_space(&e, 3 << 30)
            .unwrap()
            .contains("Test needs about 4.0 GB"));
        assert!(too_little_space(&e, 4 << 30).is_none());
    }

    #[test]
    fn free_space_is_found_for_a_real_folder() {
        assert!(free_space(&std::env::temp_dir()).is_some_and(|b| b > 0));
    }

    #[test]
    fn a_matching_archive_is_unpacked_and_the_archive_removed() {
        let root = tempfile::tempdir().unwrap();
        let zip = root.path().join("a.zip");
        let sha = zip_of(&zip, &[("bin/server.exe", b"x")]);
        let e = engine(&sha);
        finish(root.path(), &e, &zip, &sha).unwrap();
        assert!(is_installed(root.path(), &e));
        assert!(!zip.exists());
    }

    #[test]
    fn a_changed_download_is_refused_and_nothing_is_unpacked() {
        let root = tempfile::tempdir().unwrap();
        let zip = root.path().join("a.zip");
        let sha = zip_of(&zip, &[("bin/server.exe", b"x")]);
        let e = engine(&sha);
        let err = finish(root.path(), &e, &zip, &"0".repeat(64)).unwrap_err();
        assert!(err.contains("checksum"));
        assert!(!is_installed(root.path(), &e));
        assert!(!zip.exists());
    }

    #[test]
    fn an_archive_without_the_server_is_not_installed() {
        let root = tempfile::tempdir().unwrap();
        let zip = root.path().join("a.zip");
        let sha = zip_of(&zip, &[("readme.txt", b"x")]);
        let e = engine(&sha);
        assert!(finish(root.path(), &e, &zip, &sha).is_err());
        assert!(!engine_dir(root.path(), &e).exists());
    }

    #[test]
    fn an_entry_that_escapes_the_folder_is_not_written() {
        let root = tempfile::tempdir().unwrap();
        let zip = root.path().join("a.zip");
        let sha = zip_of(&zip, &[("bin/server.exe", b"x"), ("../evil.txt", b"x")]);
        let e = engine(&sha);
        let _ = finish(root.path(), &e, &zip, &sha);
        assert!(!root.path().join("engines").join("evil.txt").exists());
        assert!(!root.path().join("evil.txt").exists());
    }
}
