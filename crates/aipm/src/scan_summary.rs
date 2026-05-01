//! Default scan-summary CLI output for `aipm migrate` and `aipm lint`.
//!
//! Both commands print a single-line summary to stderr by default
//! describing what the discovery walker found. The summary makes
//! "scanned but no features matched" cases visible — closing the silent-
//! failure gap that issue #725 surfaced.
//!
//! The summary goes to stderr (not stdout) so machine-parseable
//! reporters (`json`, `ci-github`, `ci-azure`) keep stdout clean.
//! `--no-summary` suppresses the line, and it is also auto-suppressed
//! under `--log-format=json`.

use std::io::Write;

use libaipm::discovery::ScanCounts;

/// Write a single-line scan summary to `out`.
///
/// Format:
///
/// ```text
/// Scanned {N} director{y|ies} in [{sources}]; matched {format_counts(counts)}
/// ```
///
/// `format_counts` produces a comma-separated breakdown by kind, or
/// `"0 features"` when nothing matched.
///
/// # Errors
///
/// Bubbles up any I/O error from the underlying writer.
pub fn write_summary(
    out: &mut dyn Write,
    counts: ScanCounts,
    scanned_dirs: usize,
    sources: &[String],
) -> std::io::Result<()> {
    let plural = if scanned_dirs == 1 { "y" } else { "ies" };
    let sources_str = if sources.is_empty() { "(none)".to_string() } else { sources.join(", ") };
    writeln!(
        out,
        "Scanned {scanned_dirs} director{plural} in [{sources_str}]; matched {}",
        format_counts(counts),
    )
}

/// Build the trailing `"M skills, K agents, …"` string, omitting zero
/// categories. Returns `"0 features"` when the total is zero.
#[must_use]
pub fn format_counts(counts: ScanCounts) -> String {
    if counts.total() == 0 {
        return "0 features".to_string();
    }
    let parts: Vec<String> = [
        ("skill", counts.skills),
        ("agent", counts.agents),
        ("hook", counts.hooks),
        ("instruction", counts.instructions),
        ("plugin", counts.plugins),
        ("marketplace", counts.marketplaces),
        ("plugin-json", counts.plugin_jsons),
    ]
    .iter()
    .filter(|(_, n)| *n > 0)
    .map(|(label, n)| format!("{n} {label}{}", if *n == 1 { "" } else { "s" }))
    .collect();
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(skills: usize, agents: usize, hooks: usize, instructions: usize) -> ScanCounts {
        ScanCounts { skills, agents, hooks, instructions, ..ScanCounts::default() }
    }

    #[test]
    fn format_counts_zero_total() {
        assert_eq!(format_counts(ScanCounts::default()), "0 features");
    }

    #[test]
    fn format_counts_one_kind_singular() {
        assert_eq!(format_counts(sc(1, 0, 0, 0)), "1 skill");
    }

    #[test]
    fn format_counts_one_kind_plural() {
        assert_eq!(format_counts(sc(3, 0, 0, 0)), "3 skills");
    }

    #[test]
    fn format_counts_multiple_kinds_no_trailing_comma() {
        assert_eq!(format_counts(sc(3, 0, 0, 1)), "3 skills, 1 instruction");
    }

    #[test]
    fn format_counts_omits_zero_categories() {
        // 3 skills, 0 agents, 0 hooks, 1 instruction
        let formatted = format_counts(sc(3, 0, 0, 1));
        assert!(!formatted.contains("agent"));
        assert!(!formatted.contains("hook"));
    }

    #[test]
    fn format_counts_all_kinds() {
        let counts = ScanCounts {
            skills: 1,
            agents: 1,
            hooks: 1,
            instructions: 1,
            plugins: 1,
            marketplaces: 1,
            plugin_jsons: 1,
        };
        let s = format_counts(counts);
        assert!(s.contains("1 skill"));
        assert!(s.contains("1 agent"));
        assert!(s.contains("1 hook"));
        assert!(s.contains("1 instruction"));
        assert!(s.contains("1 plugin"));
        assert!(s.contains("1 marketplace"));
        assert!(s.contains("1 plugin-json"));
    }

    #[test]
    fn write_summary_singular_dir() {
        let mut buf = Vec::new();
        write_summary(&mut buf, sc(1, 0, 0, 0), 1, &[".github".to_string()]).expect("ok");
        let line = String::from_utf8(buf).expect("utf8");
        assert_eq!(line, "Scanned 1 directory in [.github]; matched 1 skill\n");
    }

    #[test]
    fn write_summary_plural_dirs() {
        let mut buf = Vec::new();
        write_summary(
            &mut buf,
            sc(3, 0, 0, 1),
            42,
            &[".github".to_string(), ".claude".to_string()],
        )
        .expect("ok");
        let line = String::from_utf8(buf).expect("utf8");
        assert_eq!(
            line,
            "Scanned 42 directories in [.github, .claude]; matched 3 skills, 1 instruction\n"
        );
    }

    #[test]
    fn write_summary_zero_dirs_zero_features() {
        let mut buf = Vec::new();
        write_summary(&mut buf, ScanCounts::default(), 0, &[]).expect("ok");
        let line = String::from_utf8(buf).expect("utf8");
        assert_eq!(line, "Scanned 0 directories in [(none)]; matched 0 features\n");
    }

    #[test]
    fn write_summary_uses_writeln_not_println() {
        // Trivial sanity: writes through the dyn Write, no global stderr/stdout.
        struct Counter(usize);
        impl Write for Counter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0 += buf.len();
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter(0);
        write_summary(&mut counter, sc(1, 0, 0, 0), 1, &[".github".to_string()]).expect("ok");
        assert!(counter.0 > 0);
    }
}
