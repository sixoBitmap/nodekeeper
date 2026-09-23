//! Log viewer backend (docs/SPEC.md item 2: "Log viewer for debug.log
//! and ord output: tail and page large files, never load a whole file;
//! search and filter"). Reads only the requested byte window via seek,
//! never the whole file into memory — a multi-GB `debug.log` must open
//! instantly (the Phase 3 [MANUAL] acceptance criterion).

use serde::Serialize;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct LogWindow {
    /// Complete lines, oldest first.
    pub lines: Vec<String>,
    /// Byte offset in the file where `lines` starts. Pass this back as
    /// `page_before`'s `end_offset` to load the window immediately
    /// before this one ("load older" / scroll up).
    #[ts(type = "number")]
    pub start_offset: u64,
    /// `true` when `start_offset` is 0 -- there is nothing older to page
    /// to, so the UI can stop offering "load older".
    pub reached_start_of_file: bool,
}

/// The last `max_bytes` of `path`, split into complete lines. A seek
/// into the middle of the file almost always lands inside a line, not
/// on a line boundary -- that leading partial line is dropped (it's
/// already available in full via the *next* `page_before` call).
pub fn tail(path: &Path, max_bytes: u64) -> io::Result<LogWindow> {
    let file_len = std::fs::metadata(path)?.len();
    read_window(path, file_len, max_bytes)
}

/// The window of up to `max_bytes` immediately before `end_offset` --
/// typically a prior call's `start_offset`, to page further back.
pub fn page_before(path: &Path, end_offset: u64, max_bytes: u64) -> io::Result<LogWindow> {
    read_window(path, end_offset, max_bytes)
}

fn read_window(path: &Path, end_offset: u64, max_bytes: u64) -> io::Result<LogWindow> {
    let mut file = File::open(path)?;
    let start = end_offset.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; (end_offset - start) as usize];
    file.read_exact(&mut buf)?;

    // Invalid UTF-8 at the split boundary (a seek can land mid-character
    // as well as mid-line) becomes U+FFFD rather than a hard failure --
    // acceptable for a log *viewer*, and confined to a boundary byte or
    // two, never the actual log content.
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.split('\n').collect();

    // A trailing '\n' (the overwhelmingly common case for a log file)
    // produces one trailing empty element from split('\n') -- drop it so
    // it isn't rendered as a blank line.
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    // Drop the leading partial line, unless this window already starts
    // at byte 0 of the file (then the first line is complete).
    let start_offset = if start > 0 && !lines.is_empty() {
        let dropped = lines.remove(0);
        start + dropped.len() as u64 + 1 // +1 for the '\n' split on
    } else {
        start
    };

    Ok(LogWindow {
        lines: lines.into_iter().map(String::from).collect(),
        start_offset,
        reached_start_of_file: start_offset == 0,
    })
}

/// Lines containing `query` (plain substring, case-sensitive), scanned
/// without ever holding the whole file in memory at once -- `BufReader`
/// streams it line by line. Stops early once `max_matches` is reached,
/// so a match near the top of a huge file doesn't require reading the
/// rest of it.
pub fn search(path: &Path, query: &str, max_matches: usize) -> io::Result<Vec<String>> {
    let reader = BufReader::new(File::open(path)?);
    let mut matches = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.contains(query) {
            matches.push(line);
            if matches.len() >= max_matches {
                break;
            }
        }
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered_lines_file(dir: &Path, count: u32) -> std::path::PathBuf {
        let path = dir.join("debug.log");
        let content: String = (0..count).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn tail_of_a_small_file_returns_every_line_and_reaches_start_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = numbered_lines_file(dir.path(), 5);

        let window = tail(&path, 4096).unwrap();
        assert_eq!(
            window.lines,
            vec!["line 0", "line 1", "line 2", "line 3", "line 4"]
        );
        assert_eq!(window.start_offset, 0);
        assert!(window.reached_start_of_file);
    }

    #[test]
    fn tail_of_a_large_file_returns_only_the_newest_complete_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = numbered_lines_file(dir.path(), 1000);

        // "line 999\n" etc. are 9-10 bytes each; a small window should
        // land well short of the whole 1000-line file.
        let window = tail(&path, 200).unwrap();
        assert!(!window.reached_start_of_file);
        assert!(window.lines.len() < 1000);
        // The newest line must be the file's actual last line.
        assert_eq!(window.lines.last().unwrap(), "line 999");
        // No line in the window should be truncated (a partial line
        // would not parse back as "line <N>" for some N).
        for line in &window.lines {
            assert!(line.strip_prefix("line ").unwrap().parse::<u32>().is_ok());
        }
    }

    #[test]
    fn paging_backward_from_the_tail_reconstructs_the_whole_file_with_no_gaps_or_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let path = numbered_lines_file(dir.path(), 500);
        let original = std::fs::read_to_string(&path).unwrap();

        let mut collected: Vec<String> = Vec::new();
        let mut window = tail(&path, 300).unwrap();
        collected.splice(0..0, window.lines.clone());
        while !window.reached_start_of_file {
            window = page_before(&path, window.start_offset, 300).unwrap();
            collected.splice(0..0, window.lines.clone());
        }

        let reconstructed: String = collected.iter().map(|l| format!("{l}\n")).collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn search_finds_matching_lines_and_respects_max_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(
            &path,
            "UpdateTip: new best=abc height=1\nother line\nUpdateTip: new best=def height=2\nUpdateTip: new best=ghi height=3\n",
        )
        .unwrap();

        let all = search(&path, "UpdateTip", 100).unwrap();
        assert_eq!(all.len(), 3);

        let capped = search(&path, "UpdateTip", 2).unwrap();
        assert_eq!(capped.len(), 2);

        let none = search(&path, "nothing matches this", 100).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn tailing_a_missing_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let result = tail(&dir.path().join("does-not-exist.log"), 4096);
        assert!(result.is_err());
    }
}
