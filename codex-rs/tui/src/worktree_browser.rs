//! Local worktree discovery and explicit removal; ownership is not activity or exclusion.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_protocol::ThreadId;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub id: uuid::Uuid,
    pub cwd: PathBuf,
    pub thread_id: Option<ThreadId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub owner: Owner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadSummary {
    pub id: ThreadId,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    None,
    Unavailable(ThreadId),
    Archived(ThreadSummary),
    Resumable(ThreadSummary),
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Resume(ThreadId),
    Copy(PathBuf),
    Remove(PathBuf),
}

pub(crate) fn fetch(
    request: Request,
    codex_home: PathBuf,
    app_server: AppServerRequestHandle,
    tx: AppEventSender,
) {
    tokio::spawn(async move {
        #[cfg(test)]
        let diagnostic_started = std::time::Instant::now();
        #[cfg(test)]
        eprintln!(
            "native2-worktree fetch={} stage=list-start ms=0",
            request.id
        );
        let result = async {
            let mut entries = list(codex_home.clone(), request.cwd.clone()).await?;
            #[cfg(test)]
            eprintln!(
                "native2-worktree fetch={} stage=list-done ms={} entries={}",
                request.id,
                diagnostic_started.elapsed().as_millis(),
                entries.len()
            );
            let archive_root = codex_home.join(codex_rollout::ARCHIVED_SESSIONS_SUBDIR);
            for (_owner_index, entry) in entries.iter_mut().enumerate() {
                let Owner::Unavailable(id) = entry.owner else {
                    continue;
                };
                #[cfg(test)]
                eprintln!(
                    "native2-worktree fetch={} stage=owner-rpc-start owner={} ms={}",
                    request.id,
                    _owner_index,
                    diagnostic_started.elapsed().as_millis()
                );
                let response = app_server
                    .request_typed::<ThreadReadResponse>(ClientRequest::ThreadRead {
                        request_id: RequestId::String(uuid::Uuid::new_v4().to_string()),
                        params: ThreadReadParams {
                            thread_id: id.to_string(),
                            include_turns: false,
                        },
                    })
                    .await;
                #[cfg(test)]
                eprintln!(
                    "native2-worktree fetch={} stage=owner-rpc-done owner={} ms={} ok={}",
                    request.id,
                    _owner_index,
                    diagnostic_started.elapsed().as_millis(),
                    response.is_ok()
                );
                if let Ok(response) = response {
                    let thread = response.thread;
                    let Some(path) = thread.path.as_ref() else {
                        continue;
                    };
                    #[cfg(test)]
                    eprintln!(
                        "native2-worktree fetch={} stage=owner-path-start owner={} ms={}",
                        request.id,
                        _owner_index,
                        diagnostic_started.elapsed().as_millis()
                    );
                    let path = codex_rollout::existing_rollout_path(path).await;
                    #[cfg(test)]
                    eprintln!(
                        "native2-worktree fetch={} stage=owner-path-done owner={} ms={} exists={}",
                        request.id,
                        _owner_index,
                        diagnostic_started.elapsed().as_millis(),
                        path.is_some()
                    );
                    let Some(path) = path else {
                        continue;
                    };
                    let archived = path.starts_with(&archive_root);
                    let title = thread
                        .name
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| {
                            thread
                                .preview
                                .lines()
                                .next()
                                .filter(|preview| !preview.trim().is_empty())
                                .unwrap_or("Untitled conversation")
                                .to_string()
                        });
                    let title = if title.chars().count() > 80 {
                        format!("{}…", title.chars().take(79).collect::<String>())
                    } else {
                        title
                    };
                    let summary = ThreadSummary {
                        id,
                        title,
                        updated_at: thread.updated_at,
                    };
                    entry.owner = if archived {
                        Owner::Archived(summary)
                    } else {
                        Owner::Resumable(summary)
                    };
                }
            }
            anyhow::Ok(entries)
        }
        .await
        .map_err(|error| error.to_string());
        #[cfg(test)]
        eprintln!(
            "native2-worktree fetch={} stage=complete ms={} ok={}",
            request.id,
            diagnostic_started.elapsed().as_millis(),
            result.is_ok()
        );
        tx.send(AppEvent::ManagedWorktreesLoaded { request, result });
    });
}

pub(crate) async fn list(codex_home: PathBuf, cwd: PathBuf) -> anyhow::Result<Vec<Entry>> {
    #[cfg(test)]
    let diagnostic_started = std::time::Instant::now();
    #[cfg(test)]
    eprintln!("native2-worktree-list stage=config-start ms=0");
    let host = crate::legacy_core::config::load_config_toml_with_layer_stack(
        &codex_home,
        /*cwd*/ None,
        Vec::new(),
        codex_config::ConfigLoadOptions::default(),
    )
    .await?;
    let settings =
        codex_worktree::WorktreeSettings::for_cli(&codex_home, host.config_toml.desktop.as_ref())?;
    #[cfg(test)]
    eprintln!(
        "native2-worktree-list stage=config-done ms={}",
        diagnostic_started.elapsed().as_millis()
    );
    // Closing the popup discards its result; an already-running blocking Git call still finishes.
    tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        eprintln!(
            "native2-worktree-list stage=git-root-start ms={}",
            diagnostic_started.elapsed().as_millis()
        );
        let cwd = codex_git_utils::get_git_repo_root(&cwd).unwrap_or(cwd);
        #[cfg(test)]
        eprintln!(
            "native2-worktree-list stage=git-root-done ms={}",
            diagnostic_started.elapsed().as_millis()
        );
        let manager = codex_worktree::WorktreeManager::new(settings);
        let checkouts = manager.list(&cwd)?;
        #[cfg(test)]
        eprintln!(
            "native2-worktree-list stage=git-list-done ms={} entries={}",
            diagnostic_started.elapsed().as_millis(),
            checkouts.len()
        );
        Ok(checkouts
            .into_iter()
            .enumerate()
            .map(|(_owner_index, checkout)| {
                #[cfg(test)]
                eprintln!(
                    "native2-worktree-list stage=git-owner-start owner={} ms={}",
                    _owner_index,
                    diagnostic_started.elapsed().as_millis()
                );
                let owner = manager
                    .owner(&checkout.root)
                    .ok()
                    .flatten()
                    .and_then(|owner| ThreadId::from_string(&owner).ok())
                    .map_or(Owner::None, Owner::Unavailable);
                #[cfg(test)]
                eprintln!(
                    "native2-worktree-list stage=git-owner-done owner={} ms={}",
                    _owner_index,
                    diagnostic_started.elapsed().as_millis()
                );
                Entry {
                    owner,
                    root: checkout.root,
                    cwd: checkout.cwd,
                }
            })
            .collect())
    })
    .await?
}

pub(crate) async fn remove(
    codex_home: PathBuf,
    source_cwd: PathBuf,
    root: PathBuf,
) -> anyhow::Result<()> {
    let host = crate::legacy_core::config::load_config_toml_with_layer_stack(
        &codex_home,
        /*cwd*/ None,
        Vec::new(),
        codex_config::ConfigLoadOptions::default(),
    )
    .await?;
    let settings =
        codex_worktree::WorktreeSettings::for_cli(&codex_home, host.config_toml.desktop.as_ref())?;
    tokio::task::spawn_blocking(move || {
        codex_worktree::WorktreeManager::new(settings).remove(&source_cwd, &root)
    })
    .await?
}
