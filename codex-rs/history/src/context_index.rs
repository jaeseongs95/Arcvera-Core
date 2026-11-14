// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Host-owned, vendor-independent context lifetimes. This index contains no conversation bodies.
//!
//! Sources enter once, conservatively active. Only explicitly bound task details can leave the
//! model projection. User/control/unknown/opaque sources and incomplete tool groups stay active.

use codex_protocol::models::ResponseItem;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextTaskState {
    Active,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextTask {
    pub id: String,
    pub label: String,
    pub state: ContextTaskState,
    pub dependencies: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    User,
    Assistant,
    Control,
    Opaque,
    ToolCall,
    ToolResult,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextSource {
    pub id: String,
    pub response_item_id: Option<String>,
    pub kind: ContextSourceKind,
    pub turn_id: Option<String>,
    pub call_id: Option<String>,
    pub ordinal: u64,
    pub task_id: Option<String>,
    /// Only the host can attest that an opaque item was produced for a known focus.
    /// Legacy or unclassified summaries remain conservative roots.
    #[serde(default)]
    pub focus_scoped: bool,
    /// Parsed once at ingestion; only a direct host policy mutation call sets this flag.
    #[serde(default)]
    pub index_policy_mutation: bool,
    /// A successful host JSON acknowledgement, paired with its call before exclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_committed_revision: Option<u64>,
    /// Equality evidence only. Older checkpoints without it cannot establish body identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_sha256: Option<String>,
    /// Actual resident representation produced by a trusted host transformation. Never used
    /// to establish equality of a newly ingested original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_sha256: Option<String>,
    #[serde(default)]
    pub host_compaction_output: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextIndexOperation {
    CreateTask {
        task_id: String,
        label: String,
    },
    BindSources {
        task_id: String,
        source_ids: Vec<String>,
    },
    SetTaskState {
        task_id: String,
        state: ContextTaskState,
    },
    AddDependency {
        task_id: String,
        dependency_id: String,
    },
    RemoveDependency {
        task_id: String,
        dependency_id: String,
    },
    SetFocus {
        task_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextIndexEvent {
    pub expected_revision: u64,
    pub revision: u64,
    pub operations: Vec<ContextIndexOperation>,
    /// Host-produced migration/reset checkpoint. Native model operations cannot create this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Box<ContextIndex>>,
}

impl ContextIndexEvent {
    pub fn checkpoint(mut index: ContextIndex) -> Result<Self, ContextIndexError> {
        index.rebuild()?;
        Ok(Self {
            expected_revision: 0,
            revision: index.revision(),
            operations: Vec::new(),
            snapshot: Some(Box::new(index)),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextIndexAudit {
    pub revision: u64,
    pub operations: Vec<ContextIndexOperation>,
    pub excluded: Vec<String>,
    pub reactivated: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextIndexError(pub String);

impl fmt::Display for ContextIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ContextIndexError {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextIndex {
    revision: u64,
    next_ordinal: u64,
    sources: BTreeMap<String, ContextSource>,
    tasks: BTreeMap<String, ContextTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    focus_task_id: Option<String>,
    audit: Vec<ContextIndexAudit>,
    // Derived lookup sets are rebuilt once on restore, never by each model request.
    #[serde(skip)]
    active_sources: BTreeSet<String>,
    #[serde(skip)]
    reachable_tasks: BTreeSet<String>,
    #[serde(skip)]
    call_groups: BTreeMap<String, BTreeSet<String>>,
}

impl ContextIndex {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn sources(&self) -> &BTreeMap<String, ContextSource> {
        &self.sources
    }
    pub fn tasks(&self) -> &BTreeMap<String, ContextTask> {
        &self.tasks
    }
    pub fn focus_task_id(&self) -> Option<&str> {
        self.focus_task_id.as_deref()
    }
    pub fn audit(&self) -> &[ContextIndexAudit] {
        &self.audit
    }
    pub fn active_source_ids(&self) -> &BTreeSet<String> {
        &self.active_sources
    }
    pub fn task_is_reachable(&self, task_id: &str) -> bool {
        self.reachable_tasks.contains(task_id)
    }
    pub fn call_group_source_ids(&self, call_id: &str) -> Option<&BTreeSet<String>> {
        self.call_groups.get(call_id)
    }

    /// Unknown IDs remain visible. Missing index metadata must never erase conversation input.
    pub fn is_active(&self, source_id: &str) -> bool {
        !self.sources.contains_key(source_id) || self.active_sources.contains(source_id)
    }

    /// A stable native ID wins. ID-less legacy tool pairs use distinct IDs in one atomic group.
    /// Other legacy items remain conservative roots and cannot be selected by a guessed ID.
    pub fn source_id(item: &ResponseItem) -> Option<String> {
        item.id().map(ToString::to_string).or_else(|| {
            let (kind, call_id) = classify(item);
            call_id.map(|id| {
                format!(
                    "call:{id}:{}",
                    if kind == ContextSourceKind::ToolCall {
                        "call"
                    } else {
                        "result"
                    }
                )
            })
        })
    }

    /// O(body bytes) hashing plus O(log n) registration; does not invalidate a captured prefix merely because its live
    /// suffix grew. Policy mutations alone increment revision and fence prefix adoption.
    pub fn register(&mut self, item: &ResponseItem) -> String {
        self.register_with_provenance(item, false)
    }

    pub fn register_with_provenance(
        &mut self,
        item: &ResponseItem,
        compaction_output: bool,
    ) -> String {
        self.register_descriptor(&Self::describe(item, compaction_output))
    }

    /// Capture identity and host provenance without retaining any conversation body.
    pub fn describe(item: &ResponseItem, compaction_output: bool) -> ContextSource {
        let (mut kind, call_id) = classify(item);
        if compaction_output {
            kind = ContextSourceKind::Opaque;
        }
        let index_policy_mutation = match item {
            ResponseItem::FunctionCall {
                name,
                namespace,
                arguments,
                ..
            } if name == "context_index"
                && namespace
                    .as_deref()
                    .is_none_or(|namespace| namespace == "functions") =>
            {
                serde_json::from_str::<serde_json::Value>(arguments)
                    .ok()
                    .is_some_and(|value| {
                        value.get("action").and_then(serde_json::Value::as_str) == Some("apply")
                    })
            }
            _ => false,
        };
        let policy_committed_revision = match item {
            ResponseItem::FunctionCallOutput { output, .. } if output.success != Some(false) => {
                output
                    .text_content()
                    // Native policy acknowledgements are bounded scalars, never tool bodies.
                    .filter(|text| text.len() <= 4096)
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                    .filter(|value| {
                        value.get("committed").and_then(serde_json::Value::as_bool) == Some(true)
                    })
                    .and_then(|value| value.get("revision").and_then(serde_json::Value::as_u64))
            }
            _ => None,
        };
        ContextSource {
            id: Self::source_id(item).unwrap_or_default(),
            response_item_id: item.id().map(ToString::to_string),
            kind,
            turn_id: item.turn_id().map(str::to_owned),
            call_id,
            ordinal: 0,
            task_id: None,
            focus_scoped: false,
            index_policy_mutation,
            policy_committed_revision,
            body_sha256: serde_json::to_vec(item)
                .ok()
                .map(|bytes| format!("{:x}", Sha256::digest(bytes))),
            projection_sha256: None,
            host_compaction_output: compaction_output,
        }
    }

    /// A checkpoint contains prepared resident bytes, not another original ingestion.
    pub fn matches_checkpoint_item(
        source: &ContextSource,
        item: &ResponseItem,
        compaction_output: bool,
    ) -> bool {
        Self::matches_checkpoint_descriptor(source, &Self::describe(item, compaction_output))
    }

    fn matches_checkpoint_descriptor(source: &ContextSource, view: &ContextSource) -> bool {
        source.kind != ContextSourceKind::Unknown
            && source.body_sha256.is_some()
            && source.kind == view.kind
            && source.call_id == view.call_id
            && source.host_compaction_output == view.host_compaction_output
            && view.body_sha256.is_some()
            && (source.body_sha256 == view.body_sha256
                || source.projection_sha256.is_some()
                    && source.projection_sha256 == view.body_sha256)
    }

    pub fn register_checkpoint_item(&mut self, item: &ResponseItem, compaction_output: bool) {
        if Self::source_id(item)
            .and_then(|id| self.sources.get(&id))
            .is_some_and(|source| Self::matches_checkpoint_item(source, item, compaction_output))
        {
            return;
        }
        self.register_with_provenance(item, compaction_output);
    }

    /// Reconcile prepared checkpoint residents after rebuild, never raw ingestion or rollback
    /// reset. Original/view hashes identify one anonymous descriptor, consumed once per resident.
    pub fn reconcile_checkpoint_descriptors<'a>(
        &mut self,
        residents: impl IntoIterator<Item = &'a ContextSource>,
    ) {
        let mut anonymous = BTreeMap::<(String, bool), Vec<String>>::new();
        for source in self.sources.values().filter(|source| {
            source.response_item_id.is_none()
                && source.call_id.is_none()
                && source.body_sha256.is_some()
        }) {
            for digest in [&source.body_sha256, &source.projection_sha256]
                .into_iter()
                .flatten()
            {
                anonymous
                    .entry((digest.clone(), source.host_compaction_output))
                    .or_default()
                    .push(source.id.clone());
            }
        }
        let mut consumed = BTreeSet::new();
        for view in residents {
            if view.response_item_id.is_some() || view.call_id.is_some() {
                if !self
                    .sources
                    .get(&view.id)
                    .is_some_and(|source| Self::matches_checkpoint_descriptor(source, view))
                {
                    self.register_descriptor(view);
                }
                continue;
            }
            let mut matched = false;
            if let Some(candidates) = view.body_sha256.as_ref().and_then(|digest| {
                anonymous.get_mut(&(digest.clone(), view.host_compaction_output))
            }) {
                while let Some(id) = candidates.pop() {
                    if !consumed.contains(&id)
                        && Self::matches_checkpoint_descriptor(&self.sources[&id], view)
                    {
                        consumed.insert(id);
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                self.register_descriptor(view);
            }
        }
    }

    /// Called only after the host has transformed a known original or an exact carried view.
    pub fn note_prepared(&mut self, id: &str, item: &ResponseItem) {
        if let Some(source) = self.sources.get_mut(id)
            && source.kind != ContextSourceKind::Unknown
            && source.body_sha256.is_some()
        {
            source.projection_sha256 =
                Self::describe(item, source.host_compaction_output).body_sha256;
        }
    }

    /// General snapshots must agree with resident ORIGINAL evidence, not its truncated view.
    pub fn reconcile_original_descriptor(&mut self, original: &ContextSource) {
        if self.sources.contains_key(&original.id) {
            self.register_descriptor(original);
        } else {
            let mut unknown = original.clone();
            unknown.kind = ContextSourceKind::Unknown;
            unknown.task_id = None;
            unknown.focus_scoped = false;
            unknown.index_policy_mutation = false;
            unknown.policy_committed_revision = None;
            unknown.body_sha256 = None;
            unknown.projection_sha256 = None;
            self.register_descriptor(&unknown);
        }
    }

    /// Re-register a body-free original after destructive history reset. Previous task policy,
    /// ordinal and derived scope are deliberately discarded, matching live ContextManager reset.
    pub fn register_descriptor(&mut self, descriptor: &ContextSource) -> String {
        let id = if descriptor.id.is_empty()
            || (descriptor.response_item_id.is_none() && descriptor.call_id.is_none())
        {
            format!("<legacy>:{}", self.next_ordinal)
        } else {
            descriptor.id.clone()
        };
        let kind = descriptor.kind;
        let call_id = descriptor.call_id.clone();
        if let Some(existing) = self.sources.get_mut(&id) {
            // A conflicting reused identity must never let an earlier completed binding hide a
            // later user/control item. Keep it unclassified rather than guessing equivalence.
            if existing.kind != kind
                || existing.call_id != call_id
                || existing.index_policy_mutation != descriptor.index_policy_mutation
                || existing.policy_committed_revision != descriptor.policy_committed_revision
                || existing.host_compaction_output != descriptor.host_compaction_output
                || existing.body_sha256.is_none()
                || existing.body_sha256 != descriptor.body_sha256
            {
                existing.kind = ContextSourceKind::Unknown;
                existing.task_id = None;
                existing.focus_scoped = false;
                existing.index_policy_mutation = false;
                existing.policy_committed_revision = None;
                existing.body_sha256 = None;
                existing.projection_sha256 = None;
                // A collision changes group completeness/classification too. Refresh every
                // peer before returning so cached projection agrees with active_reason.
                let affected_call = existing.call_id.clone();
                self.active_sources.insert(id.clone());
                if let Some(call) = affected_call {
                    let members = self.call_groups.get(&call).cloned().unwrap_or_default();
                    for member in members {
                        self.refresh_source(&member);
                    }
                }
            } else if descriptor.projection_sha256.is_some() {
                existing.projection_sha256 = descriptor.projection_sha256.clone();
            }
            return id;
        }
        let task_id = (kind != ContextSourceKind::Unknown)
            .then(|| {
                call_id
                    .as_ref()
                    .and_then(|call| self.call_groups.get(call))
                    .and_then(|members| {
                        members
                            .iter()
                            .find_map(|member| self.sources.get(member)?.task_id.clone())
                    })
                    .or_else(|| {
                        matches!(
                            kind,
                            ContextSourceKind::Assistant
                                | ContextSourceKind::ToolCall
                                | ContextSourceKind::ToolResult
                                | ContextSourceKind::Opaque
                        )
                        .then(|| self.focus_task_id.clone())
                        .flatten()
                    })
            })
            .flatten();
        let source = ContextSource {
            id: id.clone(),
            response_item_id: descriptor.response_item_id.clone(),
            kind,
            turn_id: descriptor.turn_id.clone(),
            call_id: call_id.clone(),
            ordinal: self.next_ordinal,
            focus_scoped: kind == ContextSourceKind::Opaque && task_id.is_some(),
            index_policy_mutation: descriptor.index_policy_mutation,
            policy_committed_revision: descriptor.policy_committed_revision,
            body_sha256: descriptor.body_sha256.clone(),
            projection_sha256: descriptor.projection_sha256.clone(),
            host_compaction_output: descriptor.host_compaction_output,
            task_id,
        };
        self.next_ordinal = self.next_ordinal.saturating_add(1);
        self.sources.insert(id.clone(), source);
        if let Some(call) = call_id {
            self.call_groups
                .entry(call.clone())
                .or_default()
                .insert(id.clone());
            let members = self.call_groups[&call].clone();
            for member in members {
                self.refresh_source(&member);
            }
        } else {
            self.refresh_source(&id);
        }
        id
    }

    /// Validation occurs on a candidate. Any error, including a later operation in a batch,
    /// leaves sources, tasks, revisions, active sets and audit records unchanged.
    pub fn apply(
        &mut self,
        expected_revision: u64,
        operations: Vec<ContextIndexOperation>,
    ) -> Result<ContextIndexEvent, ContextIndexError> {
        if expected_revision != self.revision {
            return Err(error("stale context index revision"));
        }
        if operations.is_empty() || operations.len() > 128 {
            return Err(error("expected 1..128 context index operations"));
        }
        let mut candidate = self.clone();
        for operation in &operations {
            candidate.apply_one(operation)?;
        }
        candidate.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| error("context index revision exhausted"))?;
        candidate.rebuild()?;
        candidate.audit.push(ContextIndexAudit {
            revision: candidate.revision,
            operations: operations.clone(),
            excluded: self
                .active_sources
                .difference(&candidate.active_sources)
                .cloned()
                .collect(),
            reactivated: candidate
                .active_sources
                .difference(&self.active_sources)
                .cloned()
                .collect(),
        });
        let event = ContextIndexEvent {
            expected_revision,
            revision: candidate.revision,
            operations,
            snapshot: None,
        };
        *self = candidate;
        Ok(event)
    }

    pub fn replay(&mut self, event: &ContextIndexEvent) -> Result<(), ContextIndexError> {
        if let Some(snapshot) = &event.snapshot {
            if event.expected_revision != 0
                || event.revision != snapshot.revision()
                || !event.operations.is_empty()
            {
                return Err(error("invalid context index checkpoint event"));
            }
            let mut restored = snapshot.as_ref().clone();
            restored.rebuild()?;
            *self = restored;
            return Ok(());
        }
        if event.expected_revision.checked_add(1) != Some(event.revision) {
            return Err(error("invalid context index event revision"));
        }
        self.apply(event.expected_revision, event.operations.clone())
            .map(|_| ())
    }

    /// Restore validates referential integrity before publishing derived lookup sets.
    pub fn rebuild(&mut self) -> Result<(), ContextIndexError> {
        for (id, task) in &self.tasks {
            check_id(id)?;
            if id != &task.id
                || task
                    .dependencies
                    .iter()
                    .any(|dep| !self.tasks.contains_key(dep))
            {
                return Err(error("invalid context task reference"));
            }
        }
        for (id, source) in &self.sources {
            if id != &source.id
                || source.ordinal >= self.next_ordinal
                || (source.index_policy_mutation && source.kind != ContextSourceKind::ToolCall)
                || (source.policy_committed_revision.is_some()
                    && source.kind != ContextSourceKind::ToolResult)
                || source
                    .task_id
                    .as_ref()
                    .is_some_and(|task| !self.tasks.contains_key(task))
            {
                return Err(error("invalid context source reference"));
            }
        }
        let mut reachable = BTreeSet::new();
        let mut pending = if let Some(focus) = &self.focus_task_id {
            if !self.tasks.contains_key(focus) {
                return Err(error("unknown focused context task"));
            }
            vec![focus.clone()]
        } else {
            self.tasks
                .values()
                .filter(|task| task.state == ContextTaskState::Active)
                .map(|task| task.id.clone())
                .collect::<Vec<_>>()
        };
        while let Some(id) = pending.pop() {
            if reachable.insert(id.clone()) {
                pending.extend(self.tasks[&id].dependencies.iter().cloned());
            }
        }
        self.reachable_tasks = reachable;
        self.call_groups.clear();
        for source in self.sources.values() {
            if let Some(call) = &source.call_id {
                self.call_groups
                    .entry(call.clone())
                    .or_default()
                    .insert(source.id.clone());
            }
        }
        // A malformed persisted snapshot cannot split a call/result group between tasks.
        for members in self.call_groups.values() {
            let mut owner = None;
            for id in members {
                if let Some(task) = &self.sources[id].task_id {
                    if owner.is_some_and(|owner| owner != task) {
                        return Err(error("conflicting context tool group owners"));
                    }
                    owner = Some(task);
                }
            }
        }
        self.active_sources.clear();
        for id in self.sources.keys() {
            if self.active_reason(id).is_some() {
                self.active_sources.insert(id.clone());
            }
        }
        Ok(())
    }

    /// Model-visible reasons explain both roots and exclusions without exposing source bodies.
    pub fn active_reason(&self, id: &str) -> Option<&'static str> {
        let Some(source) = self.sources.get(id) else {
            return Some("unknown_source");
        };
        if source.response_item_id.is_none() && source.call_id.is_none() {
            return Some("legacy_identity");
        }
        match source.kind {
            ContextSourceKind::User => return Some("user_instruction"),
            ContextSourceKind::Control => return Some("persistent_control"),
            ContextSourceKind::Opaque if !source.focus_scoped => return Some("opaque_context"),
            ContextSourceKind::Unknown => return Some("unknown_kind"),
            _ => {}
        }
        if matches!(
            source.kind,
            ContextSourceKind::ToolCall | ContextSourceKind::ToolResult
        ) && source.call_id.is_none()
        {
            return Some("unpaired_tool_source");
        }
        if let Some(call) = &source.call_id {
            let members = self.call_groups.get(call)?;
            let has_call = members
                .iter()
                .any(|id| self.sources[id].kind == ContextSourceKind::ToolCall);
            let has_result = members
                .iter()
                .any(|id| self.sources[id].kind == ContextSourceKind::ToolResult);
            if !has_call || !has_result {
                return Some("pending_tool_group");
            }
            // The committed policy already lives in the host ledger/manifest. Its raw mutation
            // arguments may mention unrelated tasks and must not be resent with every request.
            // A result alone, a failed apply, and an enclosing execute call never qualify.
            if members.iter().all(|id| match self.sources[id].kind {
                ContextSourceKind::ToolCall => self.sources[id].index_policy_mutation,
                ContextSourceKind::ToolResult => {
                    self.sources[id].policy_committed_revision.is_some()
                }
                _ => false,
            }) {
                return None;
            }
            if members.iter().any(|id| self.sources[id].task_id.is_none()) {
                return Some("unclassified_tool_group");
            }
        }
        match &source.task_id {
            None => Some("unclassified"),
            Some(task) if self.reachable_tasks.contains(task) => Some("reachable_task"),
            Some(_) => None,
        }
    }

    fn refresh_source(&mut self, id: &str) {
        if self.active_reason(id).is_some() {
            self.active_sources.insert(id.to_owned());
        } else {
            self.active_sources.remove(id);
        }
    }

    fn apply_one(&mut self, operation: &ContextIndexOperation) -> Result<(), ContextIndexError> {
        match operation {
            ContextIndexOperation::CreateTask { task_id, label } => {
                check_id(task_id)?;
                if label.len() > 1024 || self.tasks.contains_key(task_id) {
                    return Err(error("invalid label or duplicate task"));
                }
                self.tasks.insert(
                    task_id.clone(),
                    ContextTask {
                        id: task_id.clone(),
                        label: label.clone(),
                        state: ContextTaskState::Active,
                        dependencies: BTreeSet::new(),
                    },
                );
            }
            ContextIndexOperation::BindSources {
                task_id,
                source_ids,
            } => {
                if !self.tasks.contains_key(task_id) {
                    return Err(error("unknown context task"));
                }
                if source_ids.is_empty() || source_ids.len() > 1024 {
                    return Err(error("expected 1..1024 source IDs"));
                }
                let mut bound = BTreeSet::new();
                for id in source_ids {
                    let source = self
                        .sources
                        .get(id)
                        .ok_or_else(|| error("unknown context source"))?;
                    bound.insert(id.clone());
                    if let Some(call) = &source.call_id {
                        bound.extend(self.call_groups[call].iter().cloned());
                    }
                }
                for id in bound {
                    let source = self
                        .sources
                        .get_mut(&id)
                        .ok_or_else(|| error("unknown context source"))?;
                    if source
                        .task_id
                        .as_ref()
                        .is_some_and(|owner| owner != task_id)
                    {
                        return Err(error("context source already belongs to another task"));
                    }
                    source.task_id = Some(task_id.clone());
                }
            }
            ContextIndexOperation::SetTaskState { task_id, state } => {
                self.tasks
                    .get_mut(task_id)
                    .ok_or_else(|| error("unknown context task"))?
                    .state = *state;
            }
            ContextIndexOperation::AddDependency {
                task_id,
                dependency_id,
            } => {
                if !self.tasks.contains_key(dependency_id) {
                    return Err(error("unknown context dependency"));
                }
                self.tasks
                    .get_mut(task_id)
                    .ok_or_else(|| error("unknown context task"))?
                    .dependencies
                    .insert(dependency_id.clone());
            }
            ContextIndexOperation::RemoveDependency {
                task_id,
                dependency_id,
            } => {
                let task = self
                    .tasks
                    .get_mut(task_id)
                    .ok_or_else(|| error("unknown context task"))?;
                if !task.dependencies.remove(dependency_id) {
                    return Err(error("context dependency does not exist"));
                }
            }
            ContextIndexOperation::SetFocus { task_id } => {
                if !self.tasks.contains_key(task_id) {
                    return Err(error("unknown focused context task"));
                }
                self.focus_task_id = Some(task_id.clone());
            }
        }
        Ok(())
    }
}

fn classify(item: &ResponseItem) -> (ContextSourceKind, Option<String>) {
    match item {
        ResponseItem::Message { role, .. } => (
            match role.as_str() {
                "user" => ContextSourceKind::User,
                "assistant" => ContextSourceKind::Assistant,
                "system" | "developer" => ContextSourceKind::Control,
                _ => ContextSourceKind::Unknown,
            },
            None,
        ),
        ResponseItem::FunctionCall { call_id, .. }
        | ResponseItem::CustomToolCall { call_id, .. } => {
            (ContextSourceKind::ToolCall, Some(call_id.clone()))
        }
        ResponseItem::LocalShellCall { call_id, .. }
        | ResponseItem::ToolSearchCall { call_id, .. } => {
            (ContextSourceKind::ToolCall, call_id.clone())
        }
        ResponseItem::FunctionCallOutput { call_id, .. }
        | ResponseItem::ToolSearchOutput { call_id, .. } => {
            (ContextSourceKind::ToolResult, call_id.clone())
        }
        ResponseItem::CustomToolCallOutput { call_id, .. } => {
            (ContextSourceKind::ToolResult, Some(call_id.clone()))
        }
        ResponseItem::Compaction { .. }
        | ResponseItem::ContextCompaction { .. }
        | ResponseItem::Reasoning { .. } => (ContextSourceKind::Opaque, None),
        _ => (ContextSourceKind::Unknown, None),
    }
}

fn check_id(id: &str) -> Result<(), ContextIndexError> {
    if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
        return Err(error("invalid context task ID"));
    }
    Ok(())
}

fn error(message: &str) -> ContextIndexError {
    ContextIndexError(message.to_owned())
}

#[cfg(test)]
#[path = "context_index_tests.rs"]
mod tests;
