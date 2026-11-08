use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use crate::MAX_SEARCH_RESULTS;
use crate::backend::MemoriesBackendError;
use crate::backend::MemorySearchMatch;
use crate::backend::SearchMatchMode;
use crate::backend::SearchMemoriesRequest;
use crate::backend::SearchMemoriesResponse;

use super::LocalMemoriesBackend;
use super::path::display_relative_path;
use super::path::is_hidden_path;
use super::path::read_sorted_dir_entries;
use super::path::reject_symlink;

pub(super) async fn search(
    backend: &LocalMemoriesBackend,
    request: SearchMemoriesRequest,
) -> Result<SearchMemoriesResponse, MemoriesBackendError> {
    let queries = request
        .queries
        .iter()
        .map(|query| query.trim().to_string())
        .collect::<Vec<_>>();
    if queries.is_empty() || queries.iter().any(std::string::String::is_empty) {
        return Err(MemoriesBackendError::EmptyQuery);
    }
    if matches!(
        request.match_mode,
        SearchMatchMode::AllWithinLines { line_count: 0 }
    ) {
        return Err(MemoriesBackendError::InvalidMatchWindow);
    }

    let max_results = request.max_results.min(MAX_SEARCH_RESULTS);
    let start = backend.resolve_scoped_path(request.path.as_deref()).await?;
    let start_index = match request.cursor.as_deref() {
        Some(cursor) => cursor.parse::<usize>().map_err(|_| {
            MemoriesBackendError::invalid_cursor(cursor, "must be a non-negative integer")
        })?,
        None => 0,
    };
    let Some(metadata) = LocalMemoriesBackend::metadata_or_none(&start).await? else {
        return Err(MemoriesBackendError::NotFound {
            path: request.path.unwrap_or_default(),
        });
    };
    reject_symlink(&display_relative_path(&backend.root, &start), &metadata)?;

    let matcher = SearchMatcher::new(
        queries.clone(),
        request.match_mode.clone(),
        request.case_sensitive,
        request.normalized,
    )?;
    let mut matches = Vec::new();
    search_entries(
        &backend.root,
        &start,
        &metadata,
        &matcher,
        request.context_lines,
        &mut matches,
    )
    .await?;
    finish_search(request, queries, matches, start_index, max_results)
}

fn finish_search(
    request: SearchMemoriesRequest,
    queries: Vec<String>,
    mut matches: Vec<MemorySearchMatch>,
    start_index: usize,
    max_results: usize,
) -> Result<SearchMemoriesResponse, MemoriesBackendError> {
    matches.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.match_line_number.cmp(&right.match_line_number))
    });
    if start_index > matches.len() {
        return Err(MemoriesBackendError::invalid_cursor(
            start_index.to_string(),
            "exceeds result count",
        ));
    }
    let end_index = start_index.saturating_add(max_results).min(matches.len());
    let next_cursor = (end_index < matches.len()).then(|| end_index.to_string());
    let truncated = next_cursor.is_some();
    Ok(SearchMemoriesResponse {
        queries,
        match_mode: request.match_mode,
        path: request.path,
        matches: matches.drain(start_index..end_index).collect(),
        next_cursor,
        truncated,
        scan_complete: None,
        scan_stop_reason: None,
    })
}

async fn search_entries(
    root: &Path,
    current: &Path,
    current_metadata: &std::fs::Metadata,
    matcher: &SearchMatcher,
    context_lines: usize,
    matches: &mut Vec<MemorySearchMatch>,
) -> Result<(), MemoriesBackendError> {
    if current_metadata.is_file() {
        search_file(root, current, matcher, context_lines, matches).await?;
        return Ok(());
    }
    if !current_metadata.is_dir() {
        return Ok(());
    }

    let mut pending = vec![current.to_path_buf()];
    while let Some(dir_path) = pending.pop() {
        for (path, _) in read_sorted_dir_entries(&dir_path).await? {
            if is_hidden_path(&path) {
                continue;
            }
            let Some(metadata) = LocalMemoriesBackend::metadata_or_none(&path).await? else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                search_file(root, &path, matcher, context_lines, matches).await?;
            }
        }
    }

    Ok(())
}

async fn search_file(
    root: &Path,
    path: &Path,
    matcher: &SearchMatcher,
    context_lines: usize,
    matches: &mut Vec<MemorySearchMatch>,
) -> Result<(), MemoriesBackendError> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    search_text(root, path, &content, matcher, context_lines, matches);
    Ok(())
}

fn search_text(
    root: &Path,
    path: &Path,
    content: &str,
    matcher: &SearchMatcher,
    context_lines: usize,
    matches: &mut Vec<MemorySearchMatch>,
) {
    let lines = content.lines().collect::<Vec<_>>();
    visit_matches(&lines, matcher, |start, end, flags| {
        matches.push(build_search_match(
            root,
            path,
            &lines,
            start,
            end,
            context_lines,
            matcher.matched_queries(flags),
        ));
        true
    });
}

fn visit_matches(
    lines: &[&str],
    matcher: &SearchMatcher,
    mut emit: impl FnMut(usize, usize, &[bool]) -> bool,
) -> bool {
    let line_matches = lines
        .iter()
        .map(|line| matcher.matched_query_flags(line))
        .collect::<Vec<_>>();
    match &matcher.match_mode {
        SearchMatchMode::Any => {
            for (idx, matched_query_flags) in line_matches.iter().enumerate() {
                if matched_query_flags.iter().any(|matched| *matched) {
                    if !emit(idx, idx, matched_query_flags) {
                        return false;
                    }
                }
            }
        }
        SearchMatchMode::AllOnSameLine => {
            for (idx, matched_query_flags) in line_matches.iter().enumerate() {
                if matched_query_flags.iter().all(|matched| *matched) {
                    if !emit(idx, idx, matched_query_flags) {
                        return false;
                    }
                }
            }
        }
        SearchMatchMode::AllWithinLines { line_count } => {
            // For increasing starts, the first valid end is nondecreasing:
            // removing a prefix cannot make a match finish earlier. Therefore
            // a minimal window contains another iff they share an end; keep
            // the latest start for that end. This preserves the prior minimal
            // window oracle without retaining all windows or comparing O(n^2).
            let mut pending: Option<(usize, usize, Vec<bool>)> = None;
            for start_index in 0..lines.len() {
                if !line_matches[start_index].iter().any(|matched| *matched) {
                    continue;
                }
                let last_allowed_index = start_index
                    .saturating_add(line_count.saturating_sub(1))
                    .min(lines.len().saturating_sub(1));
                let mut matched_query_flags = vec![false; matcher.queries.len()];
                for (end_index, line_match_flags) in line_matches
                    .iter()
                    .enumerate()
                    .take(last_allowed_index + 1)
                    .skip(start_index)
                {
                    for (idx, matched) in line_match_flags.iter().enumerate() {
                        matched_query_flags[idx] |= matched;
                    }
                    if matched_query_flags.iter().all(|matched| *matched) {
                        if let Some((start, end, flags)) = pending.take() {
                            if end != end_index && !emit(start, end, &flags) {
                                return false;
                            }
                        }
                        pending = Some((start_index, end_index, matched_query_flags));
                        break;
                    }
                }
            }
            if let Some((start, end, flags)) = pending {
                return emit(start, end, &flags);
            }
        }
    }
    true
}

pub(super) fn search_snapshot(
    root: &Path,
    artifacts: &BTreeMap<String, Vec<u8>>,
    request: SearchMemoriesRequest,
) -> Result<SearchMemoriesResponse, MemoriesBackendError> {
    const OUTPUT_BUDGET: usize = 32 * 1024;
    const MAX_SCAN_LINES: usize = 16_384;
    const MAX_CURSOR: usize = 4_096;
    let unavailable = |reason: &str| MemoriesBackendError::MemoryUnavailable {
        reason: reason.to_string(),
    };
    let queries = request
        .queries
        .iter()
        .map(|query| query.trim().to_string())
        .collect::<Vec<_>>();
    if queries.is_empty() || queries.iter().any(String::is_empty) {
        return Err(MemoriesBackendError::EmptyQuery);
    }
    if matches!(
        request.match_mode,
        SearchMatchMode::AllWithinLines { line_count: 0 }
    ) {
        return Err(MemoriesBackendError::InvalidMatchWindow);
    }
    if queries.len() > 16
        || queries
            .iter()
            .map(String::len)
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|bytes| bytes > 4_096)
        || request.context_lines > 256
        || matches!(request.match_mode, SearchMatchMode::AllWithinLines { line_count } if line_count > 256)
    {
        return Err(unavailable(
            "versioned search request exceeds query/window budget; narrow the request",
        ));
    }
    let start_index = match request.cursor.as_deref() {
        Some(cursor) => cursor.parse::<usize>().map_err(|_| {
            MemoriesBackendError::invalid_cursor(cursor, "must be a non-negative integer")
        })?,
        None => 0,
    };
    if start_index > MAX_CURSOR {
        return Err(MemoriesBackendError::invalid_cursor(
            start_index.to_string(),
            "exceeds versioned scan budget; narrow the path or query",
        ));
    }
    let matcher = SearchMatcher::new(
        queries.clone(),
        request.match_mode.clone(),
        request.case_sensitive,
        request.normalized,
    )?;
    let scope = request.path.as_deref().unwrap_or("");
    let scope = if scope == "." { "" } else { scope };
    if !scope.is_empty()
        && !artifacts
            .keys()
            .any(|path| path == scope || path.starts_with(&format!("{scope}/")))
    {
        return Err(MemoriesBackendError::NotFound {
            path: scope.to_string(),
        });
    }
    let selected = artifacts
        .iter()
        .filter(|(path, _)| {
            scope.is_empty() || path.as_str() == scope || path.starts_with(&format!("{scope}/"))
        })
        .filter_map(|(path, bytes)| {
            std::str::from_utf8(bytes)
                .ok()
                .map(|content| (path, content))
        })
        .collect::<Vec<_>>();
    let mut line_count = 0usize;
    for (_, content) in &selected {
        line_count = line_count
            .checked_add(content.lines().count())
            .ok_or_else(|| unavailable("versioned search line count overflow"))?;
        if line_count > MAX_SCAN_LINES {
            return Err(unavailable(
                "versioned search scope exceeds line scan budget; narrow the path or use a line-limited read; total matches were not observed",
            ));
        }
    }
    let max_results = request.max_results.min(MAX_SEARCH_RESULTS);
    let mut response = SearchMemoriesResponse {
        queries,
        match_mode: request.match_mode.clone(),
        path: request.path.clone(),
        matches: Vec::new(),
        next_cursor: Some((MAX_CURSOR + MAX_SEARCH_RESULTS).to_string()),
        truncated: true,
        scan_complete: Some(false),
        scan_stop_reason: Some("cursor_budget_narrow_scope".to_string()),
    };
    let mut output_bytes = serde_json::to_vec(&response)
        .map_err(|_| unavailable("search response encoding failed"))?
        .len();
    if output_bytes > OUTPUT_BUDGET {
        return Err(unavailable(
            "search metadata exceeds output budget; narrow the request",
        ));
    }
    let mut seen = 0usize;
    let mut stop_reason = None;
    let mut oversized_first = false;
    for (path, content) in selected {
        let lines = content.lines().collect::<Vec<_>>();
        let complete = visit_matches(&lines, &matcher, |start, end, flags| {
            if seen < start_index {
                seen += 1;
                return true;
            }
            if response.matches.len() >= max_results {
                stop_reason = Some("item_limit");
                return false;
            }
            let content_start = start.saturating_sub(request.context_lines);
            let content_end = end
                .saturating_add(request.context_lines)
                .saturating_add(1)
                .min(lines.len());
            let raw_bytes =
                lines[content_start..content_end]
                    .iter()
                    .try_fold(0usize, |total, line| {
                        total
                            .checked_add(line.len())
                            .and_then(|bytes| bytes.checked_add(1))
                    });
            let remaining = OUTPUT_BUDGET.saturating_sub(output_bytes);
            if raw_bytes.is_none_or(|bytes| bytes > remaining) {
                oversized_first = response.matches.is_empty();
                stop_reason = Some("output_budget");
                return false;
            }
            // Construct at most one bounded candidate. Never accumulate a full
            // result vector and then truncate it; UTF-8 text stays complete.
            let candidate = build_search_match(
                root,
                &root.join(path),
                &lines,
                start,
                end,
                request.context_lines,
                matcher.matched_queries(flags),
            );
            let candidate_bytes = match serde_json::to_vec(&candidate) {
                Ok(bytes) => bytes.len().saturating_add(1),
                Err(_) => {
                    oversized_first = true;
                    stop_reason = Some("encoding");
                    return false;
                }
            };
            if candidate_bytes > remaining {
                oversized_first = response.matches.is_empty();
                stop_reason = Some("output_budget");
                return false;
            }
            output_bytes += candidate_bytes;
            response.matches.push(candidate);
            seen += 1;
            true
        });
        if !complete {
            break;
        }
    }
    if oversized_first {
        return Err(unavailable(
            "one search match exceeds the output budget; reduce context_lines or use a line-limited read; total matches were not observed",
        ));
    }
    if seen < start_index {
        return Err(MemoriesBackendError::invalid_cursor(
            start_index.to_string(),
            "exceeds result count",
        ));
    }
    response.truncated = stop_reason.is_some();
    response.scan_complete = Some(stop_reason.is_none());
    let next_index = start_index
        .checked_add(response.matches.len())
        .ok_or_else(|| unavailable("search cursor overflow"))?;
    response.scan_stop_reason = stop_reason.map(|reason| {
        if next_index > MAX_CURSOR {
            "cursor_budget_narrow_scope".to_string()
        } else {
            reason.to_string()
        }
    });
    response.next_cursor =
        stop_reason.and_then(|_| (next_index <= MAX_CURSOR).then(|| next_index.to_string()));
    if serde_json::to_vec(&response)
        .map_err(|_| unavailable("search response encoding failed"))?
        .len()
        > OUTPUT_BUDGET
    {
        return Err(unavailable(
            "search response exceeds output budget; narrow the request",
        ));
    }
    Ok(response)
}

fn build_search_match(
    root: &Path,
    path: &Path,
    lines: &[&str],
    match_start_index: usize,
    match_end_index: usize,
    context_lines: usize,
    matched_queries: Vec<String>,
) -> MemorySearchMatch {
    let content_start_index = match_start_index.saturating_sub(context_lines);
    let content_end_index = match_end_index
        .saturating_add(context_lines)
        .saturating_add(1)
        .min(lines.len());
    MemorySearchMatch {
        path: display_relative_path(root, path),
        match_line_number: match_start_index + 1,
        content_start_line_number: content_start_index + 1,
        content: lines[content_start_index..content_end_index].join("\n"),
        matched_queries,
    }
}

struct SearchMatcher {
    queries: Vec<String>,
    prepared_queries: Vec<String>,
    comparison: SearchComparison,
    match_mode: SearchMatchMode,
}

impl SearchMatcher {
    fn new(
        queries: Vec<String>,
        match_mode: SearchMatchMode,
        case_sensitive: bool,
        normalized: bool,
    ) -> Result<Self, MemoriesBackendError> {
        let comparison = SearchComparison::new(case_sensitive, normalized);
        let prepared_queries = queries
            .iter()
            .map(|query| comparison.prepare(query))
            .map(Cow::into_owned)
            .collect::<Vec<_>>();
        if prepared_queries.iter().any(std::string::String::is_empty) {
            return Err(MemoriesBackendError::EmptyQuery);
        }
        Ok(Self {
            queries,
            prepared_queries,
            comparison,
            match_mode,
        })
    }

    fn matched_query_flags(&self, line: &str) -> Vec<bool> {
        let line = self.comparison.prepare(line);
        self.prepared_queries
            .iter()
            .map(|query| line.as_ref().contains(query))
            .collect()
    }

    fn matched_queries(&self, matched_query_flags: &[bool]) -> Vec<String> {
        self.queries
            .iter()
            .zip(matched_query_flags)
            .filter_map(|(query, matched)| matched.then_some(query.clone()))
            .collect()
    }
}

#[derive(Clone, Copy)]
struct SearchComparison {
    case_sensitive: bool,
    normalized: bool,
}

impl SearchComparison {
    fn new(case_sensitive: bool, normalized: bool) -> Self {
        Self {
            case_sensitive,
            normalized,
        }
    }

    fn prepare<'a>(self, value: &'a str) -> Cow<'a, str> {
        if self.case_sensitive && !self.normalized {
            return Cow::Borrowed(value);
        }

        let value = if self.case_sensitive {
            Cow::Borrowed(value)
        } else {
            Cow::Owned(value.to_lowercase())
        };
        if !self.normalized {
            return value;
        }

        Cow::Owned(
            value
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect::<String>(),
        )
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn request() -> SearchMemoriesRequest {
        SearchMemoriesRequest {
            queries: vec!["decision".to_string()],
            match_mode: SearchMatchMode::Any,
            path: None,
            cursor: None,
            context_lines: 0,
            case_sensitive: false,
            normalized: false,
            max_results: 200,
        }
    }

    #[test]
    fn streaming_windows_match_an_independent_minimal_interval_oracle() {
        for encoding in 0usize..4usize.pow(5) {
            let mut encoding = encoding;
            let mut lines = Vec::new();
            for _ in 0..5 {
                lines.push(["", "a", "b", "ab"][encoding % 4]);
                encoding /= 4;
            }
            for width in 1..=5 {
                let mut all = Vec::new();
                for start in 0..lines.len() {
                    for end in start..lines.len().min(start + width) {
                        if ["a", "b"]
                            .iter()
                            .all(|query| lines[start..=end].iter().any(|line| line.contains(query)))
                        {
                            all.push((start, end));
                        }
                    }
                }
                let expected = all
                    .iter()
                    .copied()
                    .filter(|(start, end)| {
                        !all.iter().any(|(inner_start, inner_end)| {
                            inner_start >= start
                                && inner_end <= end
                                && (inner_start, inner_end) != (start, end)
                        })
                    })
                    .collect::<Vec<_>>();
                let matcher = SearchMatcher::new(
                    vec!["a".to_string(), "b".to_string()],
                    SearchMatchMode::AllWithinLines { line_count: width },
                    true,
                    false,
                )
                .unwrap();
                let mut actual = Vec::new();
                assert!(visit_matches(&lines, &matcher, |start, end, _| {
                    actual.push((start, end));
                    true
                }));
                assert_eq!(actual, expected, "lines={lines:?}, width={width}");
            }
        }
    }

    #[test]
    fn versioned_pages_bound_utf8_output_before_accumulation_and_resume() {
        let mut artifacts = BTreeMap::new();
        artifacts.insert(
            "memory_summary.md".to_string(),
            "메모리 decision 근거\n".repeat(500).into_bytes(),
        );
        let first = search_snapshot(Path::new("fixture"), &artifacts, request()).unwrap();
        assert!(serde_json::to_vec(&first).unwrap().len() <= 32 * 1024);
        assert!(first.truncated);
        assert_eq!(first.scan_complete, Some(false));
        assert!(first.scan_stop_reason.is_some());
        assert!(!first.matches.is_empty());
        let next = first.next_cursor.clone().unwrap().parse::<usize>().unwrap();
        assert_eq!(first.matches.last().unwrap().match_line_number, next);
        let mut second = request();
        second.cursor = Some(next.to_string());
        let second = search_snapshot(Path::new("fixture"), &artifacts, second).unwrap();
        assert_eq!(second.matches[0].match_line_number, next + 1);
        assert!(second.matches[0].content.contains("메모리"));
    }

    #[test]
    fn versioned_search_budget_failure_is_not_reported_as_zero_matches() {
        let mut artifacts = BTreeMap::new();
        artifacts.insert(
            "memory_summary.md".to_string(),
            "decision".repeat(5_000).into_bytes(),
        );
        assert!(matches!(
            search_snapshot(Path::new("fixture"), &artifacts, request()),
            Err(MemoriesBackendError::MemoryUnavailable { .. })
        ));
        let mut huge_context = request();
        huge_context.context_lines = usize::MAX;
        assert!(matches!(
            search_snapshot(Path::new("fixture"), &artifacts, huge_context),
            Err(MemoriesBackendError::MemoryUnavailable { .. })
        ));
        let mut huge_cursor = request();
        huge_cursor.cursor = Some(usize::MAX.to_string());
        assert!(matches!(
            search_snapshot(Path::new("fixture"), &artifacts, huge_cursor),
            Err(MemoriesBackendError::InvalidCursor { .. })
        ));
        artifacts.insert(
            "memory_summary.md".to_string(),
            "decision\n".repeat(16_385).into_bytes(),
        );
        assert!(matches!(
            search_snapshot(Path::new("fixture"), &artifacts, request()),
            Err(MemoriesBackendError::MemoryUnavailable { .. })
        ));
    }
}
