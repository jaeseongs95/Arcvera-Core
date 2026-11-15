// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Opt-in, payload-free observations at the real browser opener boundary.

use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

const MAX_RECORDED_ATTEMPTS: u64 = 4096;
static DIAGNOSTICS: OnceLock<Counters> = OnceLock::new();

#[derive(Default)]
struct Counters {
    attempts: AtomicU64,
    completed: AtomicU64,
    succeeded: AtomicU64,
}

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    phase: &'static str,
    id: u64,
    ok: bool,
    attempts: u64,
    completed: u64,
    succeeded: u64,
    suppressed: u64,
}

impl Counters {
    fn snapshot(&self, phase: &'static str, id: u64, ok: bool) -> Observation {
        let attempts = self.attempts.load(Ordering::Relaxed);
        Observation {
            phase,
            id,
            ok,
            attempts,
            completed: self.completed.load(Ordering::Relaxed),
            succeeded: self.succeeded.load(Ordering::Relaxed),
            suppressed: attempts.saturating_sub(MAX_RECORDED_ATTEMPTS),
        }
    }

    fn open<T, E>(
        &self,
        opener: impl FnOnce() -> Result<T, E>,
        mut emit: impl FnMut(Observation),
    ) -> Result<T, E> {
        let id = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        // ceiling: bound diagnostic output; overflow still opens normally and invalidates
        // a complete observation run through the final suppressed count.
        if id <= MAX_RECORDED_ATTEMPTS {
            emit(self.snapshot("attempt", id, false));
        }
        let result = opener();
        if result.is_ok() {
            self.succeeded.fetch_add(1, Ordering::Relaxed);
        }
        self.completed.fetch_add(1, Ordering::Relaxed);
        if id <= MAX_RECORDED_ATTEMPTS {
            emit(self.snapshot("completion", id, result.is_ok()));
        }
        result
    }
}

fn emit(event: Observation) {
    tracing::info!(
        target: "codex_tui::link_diagnostics",
        owner = "tui",
        pid = std::process::id(),
        phase = event.phase,
        id = event.id,
        ok = event.ok,
        attempts = event.attempts,
        completed = event.completed,
        succeeded = event.succeeded,
        suppressed = event.suppressed,
        "link_open_trace"
    );
}

pub(crate) fn init() {
    if std::env::var_os("ARCVERA_LINK_DIAGNOSTICS").as_deref() == Some(std::ffi::OsStr::new("1"))
        && DIAGNOSTICS.set(Counters::default()).is_ok()
    {
        emit(
            DIAGNOSTICS
                .get()
                .expect("initialized diagnostics")
                .snapshot("init", 0, false),
        );
    }
}

pub(crate) fn finish() {
    if let Some(counters) = DIAGNOSTICS.get() {
        emit(counters.snapshot("final", 0, false));
    }
}

pub(crate) fn open(url: &str) -> std::io::Result<()> {
    open_with(DIAGNOSTICS.get(), || webbrowser::open(url), emit)
}

fn open_with<T, E>(
    counters: Option<&Counters>,
    opener: impl FnOnce() -> Result<T, E>,
    emit: impl FnMut(Observation),
) -> Result<T, E> {
    match counters {
        Some(counters) => counters.open(opener, emit),
        None => opener(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_keeps_result_and_emits_nothing() {
        let mut calls = 0;
        let result = open_with(
            None,
            || {
                calls += 1;
                Ok::<_, ()>(7)
            },
            |_| panic!("disabled"),
        );
        assert_eq!(result, Ok(7));
        assert_eq!(calls, 1);
    }

    #[test]
    fn success_and_failure_preserve_results_and_payload_free_counts() {
        let counters = Counters::default();
        let mut events = vec![counters.snapshot("init", 0, false)];
        assert_eq!(
            open_with(Some(&counters), || Ok::<_, &str>(7), |e| events.push(e)),
            Ok(7)
        );
        let secret_error = "url-and-error-must-not-be-recorded";
        assert_eq!(
            open_with(
                Some(&counters),
                || Err::<(), _>(secret_error),
                |e| events.push(e)
            ),
            Err(secret_error)
        );
        events.push(counters.snapshot("final", 0, false));
        assert_eq!(
            events.iter().map(|e| (e.phase, e.id)).collect::<Vec<_>>(),
            vec![
                ("init", 0),
                ("attempt", 1),
                ("completion", 1),
                ("attempt", 2),
                ("completion", 2),
                ("final", 0),
            ]
        );
        assert!(events[2].ok);
        assert!(!events[4].ok);
        let final_event = events.last().unwrap();
        assert_eq!(
            (
                final_event.attempts,
                final_event.completed,
                final_event.succeeded,
                final_event.suppressed
            ),
            (2, 2, 1, 0)
        );
        assert!(!format!("{events:?}").contains(secret_error));
    }

    #[test]
    fn overflow_bounds_records_without_suppressing_browser_calls() {
        let counters = Counters::default();
        let mut calls = 0;
        let mut records = 0;
        for _ in 0..MAX_RECORDED_ATTEMPTS + 1 {
            open_with(
                Some(&counters),
                || {
                    calls += 1;
                    Ok::<_, ()>(())
                },
                |_| records += 1,
            )
            .unwrap();
        }
        assert_eq!(calls, MAX_RECORDED_ATTEMPTS + 1);
        assert_eq!(records, MAX_RECORDED_ATTEMPTS * 2);
        let summary = counters.snapshot("final", 0, false);
        assert_eq!(summary.attempts, summary.completed);
        assert_eq!(summary.suppressed, 1);
    }
}
