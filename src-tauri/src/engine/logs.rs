//! Bounded, rotating log capture for one service.
//!
//! Memory is capped by a ring of the newest lines; everything else goes to rotating files on
//! disk. Lines longer than `MAX_LINE_BYTES` are truncated; registered secret values are masked
//! before anything is stored or emitted.

use serde::Serialize;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LINE_BYTES: usize = 64 * 1024;
const MASK: &str = "••••";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts_ms: u64,
    pub stream: Stream,
    pub text: String,
}

struct Inner {
    ring: VecDeque<LogLine>,
    writer: Option<BufWriter<File>>,
    size: u64,
    secrets: Vec<String>,
}

pub struct LogSink {
    dir: PathBuf,
    service_id: String,
    max_file_bytes: u64,
    max_files: usize,
    ring_lines: usize,
    inner: Mutex<Inner>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate(line: &str) -> String {
    if line.len() <= MAX_LINE_BYTES {
        return line.to_string();
    }
    let mut cut = MAX_LINE_BYTES;
    while !line.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}… [truncated {} bytes]", &line[..cut], line.len() - cut)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

impl LogSink {
    pub fn new(
        dir: PathBuf,
        service_id: &str,
        max_file_bytes: u64,
        max_files: usize,
        ring_lines: usize,
    ) -> Self {
        let _ = fs::create_dir_all(&dir);
        let current = dir.join(format!("{service_id}.log"));
        let size = fs::metadata(&current).map(|m| m.len()).unwrap_or(0);
        Self {
            dir,
            service_id: service_id.to_string(),
            max_file_bytes,
            max_files: max_files.max(1),
            ring_lines: ring_lines.max(1),
            inner: Mutex::new(Inner {
                ring: VecDeque::new(),
                writer: None,
                size,
                secrets: Vec::new(),
            }),
        }
    }

    fn current_path(&self) -> PathBuf {
        self.dir.join(format!("{}.log", self.service_id))
    }

    fn rotated_path(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{}.log.{n}", self.service_id))
    }

    /// Values to mask in everything stored or emitted from now on.
    pub fn set_secrets(&self, secrets: Vec<String>) {
        self.inner.lock().unwrap().secrets =
            secrets.into_iter().filter(|s| !s.is_empty()).collect();
    }

    pub fn push(&self, stream: Stream, line: &str) {
        let mut inner = self.inner.lock().unwrap();
        let mut text = line.to_string();
        for secret in &inner.secrets {
            text = text.replace(secret.as_str(), MASK);
        }
        let text = truncate(&text);
        let ts_ms = now_ms();
        let record = format!(
            "{ts_ms}\t{}\t{}\n",
            if stream == Stream::Stdout { 'o' } else { 'e' },
            escape(&text)
        );
        self.write_record(&mut inner, &record);
        inner.ring.push_back(LogLine {
            ts_ms,
            stream,
            text,
        });
        while inner.ring.len() > self.ring_lines {
            inner.ring.pop_front();
        }
    }

    fn write_record(&self, inner: &mut Inner, record: &str) {
        let len = record.len() as u64;
        if inner.size > 0 && inner.size + len > self.max_file_bytes {
            self.rotate(inner);
        }
        if inner.writer.is_none() {
            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.current_path())
            {
                Ok(f) => inner.writer = Some(BufWriter::new(f)),
                Err(_) => return, // logging must never take the service down
            }
        }
        if let Some(w) = inner.writer.as_mut() {
            if w.write_all(record.as_bytes()).is_ok() {
                inner.size += len;
            }
        }
    }

    fn rotate(&self, inner: &mut Inner) {
        if let Some(mut w) = inner.writer.take() {
            let _ = w.flush();
        }
        let last = self.max_files - 1;
        if last == 0 {
            let _ = fs::remove_file(self.current_path());
        } else {
            let _ = fs::remove_file(self.rotated_path(last));
            for n in (1..last).rev() {
                let _ = fs::rename(self.rotated_path(n), self.rotated_path(n + 1));
            }
            let _ = fs::rename(self.current_path(), self.rotated_path(1));
        }
        inner.size = 0;
    }

    pub fn flush(&self) {
        if let Some(w) = self.inner.lock().unwrap().writer.as_mut() {
            let _ = w.flush();
        }
    }

    /// The newest `n` lines still in memory, oldest first.
    pub fn recent(&self, n: usize) -> Vec<LogLine> {
        let inner = self.inner.lock().unwrap();
        let skip = inner.ring.len().saturating_sub(n);
        inner.ring.iter().skip(skip).cloned().collect()
    }

    /// Lines from disk, oldest first, optionally filtered by a case-insensitive substring.
    // shortcut: reads every retained file; bounded by max_files * max_file_bytes. Index or
    // seek if retention limits are raised a lot.
    pub fn read_history(&self, offset: usize, limit: usize, filter: Option<&str>) -> Vec<LogLine> {
        self.flush();
        let needle = filter.map(str::to_lowercase);
        let mut paths: Vec<PathBuf> = (1..self.max_files)
            .rev()
            .map(|n| self.rotated_path(n))
            .collect();
        paths.push(self.current_path());
        let mut out = Vec::new();
        let mut skipped = 0;
        for path in paths {
            let Ok(file) = File::open(&path) else {
                continue;
            };
            for raw in BufReader::new(file).lines().map_while(Result::ok) {
                let mut parts = raw.splitn(3, '\t');
                let (Some(ts), Some(s), Some(text)) = (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let Ok(ts_ms) = ts.parse() else { continue };
                let text = unescape(text);
                if let Some(n) = &needle {
                    if !text.to_lowercase().contains(n.as_str()) {
                        continue;
                    }
                }
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                out.push(LogLine {
                    ts_ms,
                    stream: if s == "e" {
                        Stream::Stderr
                    } else {
                        Stream::Stdout
                    },
                    text,
                });
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }
}

/// Room left for the truncation marker so a pumped line is never truncated twice.
const PUMP_CAP: usize = MAX_LINE_BYTES - 64;

/// Reads `reader` line by line into `sink`, discarding bytes beyond the cap *as they are read*,
/// so one enormous line can never be held in memory. Returns when the reader ends.
pub fn pump<R: std::io::Read>(reader: R, stream: Stream, sink: &LogSink) {
    let mut r = BufReader::with_capacity(64 * 1024, reader);
    let mut line: Vec<u8> = Vec::new();
    let mut dropped = 0usize;

    let emit = |line: &mut Vec<u8>, dropped: &mut usize| {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let mut text = String::from_utf8_lossy(line).into_owned();
        if *dropped > 0 {
            text.push_str(&format!("… [truncated {dropped} bytes]"));
        }
        sink.push(stream, &text);
        line.clear();
        *dropped = 0;
    };
    let take = |line: &mut Vec<u8>, dropped: &mut usize, chunk: &[u8]| {
        let room = PUMP_CAP.saturating_sub(line.len());
        if chunk.len() <= room {
            line.extend_from_slice(chunk);
        } else {
            line.extend_from_slice(&chunk[..room]);
            *dropped += chunk.len() - room;
        }
    };

    loop {
        let (consumed, ended_line) = match r.fill_buf() {
            Ok([]) | Err(_) => break,
            Ok(buf) => match buf.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    take(&mut line, &mut dropped, &buf[..i]);
                    (i + 1, true)
                }
                None => {
                    take(&mut line, &mut dropped, buf);
                    (buf.len(), false)
                }
            },
        };
        r.consume(consumed);
        if ended_line {
            emit(&mut line, &mut dropped);
        }
    }
    if !line.is_empty() || dropped > 0 {
        emit(&mut line, &mut dropped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink(dir: &std::path::Path, max_file: u64, max_files: usize, ring: usize) -> LogSink {
        LogSink::new(dir.to_path_buf(), "svc", max_file, max_files, ring)
    }

    fn files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("svc.log")
            })
            .collect()
    }

    #[test]
    fn the_ring_never_exceeds_its_capacity_and_keeps_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 5);
        for i in 0..100 {
            s.push(Stream::Stdout, &format!("line {i}"));
        }
        let recent = s.recent(1000);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent.last().unwrap().text, "line 99");
        assert_eq!(recent.first().unwrap().text, "line 95");
        assert_eq!(s.recent(2).len(), 2);
    }

    #[test]
    fn rotation_keeps_at_most_max_files_each_near_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 2_000, 3, 10);
        for i in 0..2_000 {
            s.push(
                Stream::Stdout,
                &format!("a reasonably long log line number {i}"),
            );
        }
        s.flush();
        let fs = files(dir.path());
        assert!(fs.len() <= 3, "{} files", fs.len());
        for f in fs {
            let len = std::fs::metadata(&f).unwrap().len();
            assert!(len <= 2_000 + 200, "{f:?} is {len} bytes");
        }
    }

    #[test]
    fn a_four_megabyte_line_is_truncated_in_memory_and_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 10);
        s.push(Stream::Stderr, &"x".repeat(4 * 1024 * 1024));
        s.flush();
        let line = &s.recent(1)[0];
        assert!(
            line.text.len() <= MAX_LINE_BYTES + 64,
            "{}",
            line.text.len()
        );
        assert!(line.text.contains("[truncated"));
        assert_eq!(line.stream, Stream::Stderr);
        let on_disk: u64 = files(dir.path())
            .iter()
            .map(|f| std::fs::metadata(f).unwrap().len())
            .sum();
        assert!(on_disk < 200_000, "{on_disk} bytes on disk");
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 10);
        s.push(Stream::Stdout, &"é".repeat(MAX_LINE_BYTES)); // 2 bytes per char
        let t = &s.recent(1)[0].text;
        assert!(t.contains("[truncated"));
        assert!(t.is_char_boundary(t.len()));
    }

    #[test]
    fn a_registered_secret_never_appears_in_memory_or_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 10);
        s.set_secrets(vec!["hunter2".into()]);
        s.push(Stream::Stdout, "connecting with password hunter2 now");
        s.flush();
        assert!(!s.recent(1)[0].text.contains("hunter2"));
        assert!(s.recent(1)[0].text.contains("••••"));
        for f in files(dir.path()) {
            let raw = std::fs::read_to_string(f).unwrap();
            assert!(!raw.contains("hunter2"));
        }
    }

    #[test]
    fn history_reads_across_rotated_files_in_order_and_filters() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 500, 5, 3);
        for i in 0..60 {
            s.push(
                if i % 2 == 0 {
                    Stream::Stdout
                } else {
                    Stream::Stderr
                },
                &format!("event {i} {}", if i % 10 == 0 { "ERROR" } else { "ok" }),
            );
        }
        let all = s.read_history(0, 1000, None);
        assert!(all.len() >= 10 && all.len() <= 60);
        let nums: Vec<u32> = all
            .iter()
            .map(|l| l.text.split(' ').nth(1).unwrap().parse().unwrap())
            .collect();
        assert!(
            nums.windows(2).all(|w| w[0] < w[1]),
            "out of order: {nums:?}"
        );
        let errors = s.read_history(0, 1000, Some("error"));
        assert!(!errors.is_empty());
        assert!(errors.iter().all(|l| l.text.contains("ERROR")));
        assert_eq!(s.read_history(2, 3, None).len(), 3);
    }

    #[test]
    fn pump_splits_lines_and_caps_a_huge_line_while_reading() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 10);
        let mut input = Vec::new();
        input.extend_from_slice(b"first\r\n");
        input.extend(std::iter::repeat_n(b'x', 5 * 1024 * 1024));
        input.extend_from_slice(b"\nlast without newline");
        pump(std::io::Cursor::new(input), Stream::Stdout, &s);
        let lines = s.recent(10);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "first");
        assert!(
            lines[1].text.len() <= MAX_LINE_BYTES,
            "{}",
            lines[1].text.len()
        );
        assert_eq!(lines[1].text.matches("[truncated").count(), 1);
        assert_eq!(lines[2].text, "last without newline");
    }

    #[test]
    fn tabs_backslashes_and_carriage_returns_survive_the_disk_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = sink(dir.path(), 1 << 20, 3, 3);
        s.push(Stream::Stdout, "a\tb\\c\rd");
        let h = s.read_history(0, 10, None);
        assert_eq!(h[0].text, "a\tb\\c\rd");
    }
}
