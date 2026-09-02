use std::collections::{HashMap, HashSet};
use std::path::Path;

use baude_core::lifecycle::{lifecycle_capability, LifecycleCapability};
use baude_core::repository::{
    CheckoutHealth, CheckoutKey, CheckoutRole, RepositoryHealth, RepositoryKey, RepositoryState,
    StandaloneKey, StandaloneLifecycle,
};
use baude_core::session::Status;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LocalRowId {
    Repository(RepositoryKey),
    Checkout(CheckoutKey),
    Standalone(StandaloneKey),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionTarget {
    Local(LocalRowId),
    Remote(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckoutDecoration {
    pub runtime_id: Option<u64>,
    pub status: Option<Status>,
    pub waiting_for_ms: u64,
    pub archived: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalStatus {
    Waiting,
    Working,
    Completed,
    Exited,
    Closed,
    Archived,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionKind {
    Repository,
    Main,
    Managed,
    External,
    Unavailable,
    Standalone,
    Remote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionSelection {
    Repository {
        available: bool,
    },
    Checkout {
        role: CheckoutRole,
        managed_by_baude: bool,
        available: bool,
    },
    Standalone {
        available: bool,
    },
    #[allow(dead_code)] // App remote dispatch consumes this in Task 07-02-02.
    Remote,
}

/// Presentation-ready actions derived only from durable selection facts,
/// optional runtime association, and explicit core lifecycle authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionView {
    pub kind: ActionKind,
    pub has_runtime: bool,
    pub capability: Option<LifecycleCapability>,
    pub can_activate_branch: bool,
    pub can_close: bool,
    pub can_remove: bool,
}

pub fn action_view(
    selection: ActionSelection,
    has_runtime: bool,
    capability: Option<LifecycleCapability>,
) -> ActionView {
    let (kind, can_activate_branch, can_close, can_remove) = match selection {
        ActionSelection::Repository { available: true } => {
            (ActionKind::Repository, true, false, false)
        }
        ActionSelection::Repository { available: false }
        | ActionSelection::Checkout {
            available: false, ..
        } => (ActionKind::Unavailable, false, false, false),
        ActionSelection::Checkout {
            role: CheckoutRole::Main,
            available: true,
            ..
        } => (ActionKind::Main, true, has_runtime, false),
        ActionSelection::Checkout {
            managed_by_baude: true,
            available: true,
            ..
        } => (ActionKind::Managed, true, has_runtime, true),
        ActionSelection::Checkout {
            available: true, ..
        } => (ActionKind::External, true, has_runtime, false),
        ActionSelection::Standalone { available: true } => {
            (ActionKind::Standalone, false, has_runtime, false)
        }
        ActionSelection::Standalone { available: false } => {
            (ActionKind::Unavailable, false, false, false)
        }
        ActionSelection::Remote => (ActionKind::Remote, false, false, false),
    };
    ActionView {
        kind,
        has_runtime,
        capability,
        can_activate_branch,
        can_close,
        can_remove,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRepositoryRow {
    pub key: RepositoryKey,
    pub name: String,
    pub display_name: String,
    pub main_path: std::path::PathBuf,
    pub health: RepositoryHealth,
    pub child_count: usize,
    pub waiting_count: usize,
    pub actions: ActionView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCheckoutRow {
    pub key: CheckoutKey,
    pub repository_key: RepositoryKey,
    pub role: CheckoutRole,
    pub managed_by_baude: bool,
    pub name: String,
    pub branch: Option<String>,
    pub runtime_id: Option<u64>,
    pub status: LocalStatus,
    pub waiting_for_ms: u64,
    pub archived: bool,
    pub health: CheckoutHealth,
    pub actions: ActionView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalStandaloneRow {
    pub key: StandaloneKey,
    pub name: String,
    pub path: std::path::PathBuf,
    pub runtime_id: Option<u64>,
    pub status: LocalStatus,
    pub waiting_for_ms: u64,
    pub archived: bool,
    pub lifecycle: StandaloneLifecycle,
    pub actions: ActionView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalRow {
    Repository(LocalRepositoryRow),
    Checkout(LocalCheckoutRow),
    Standalone(LocalStandaloneRow),
}

impl LocalRow {
    pub fn id(&self) -> LocalRowId {
        match self {
            Self::Repository(row) => LocalRowId::Repository(row.key),
            Self::Checkout(row) => LocalRowId::Checkout(row.key),
            Self::Standalone(row) => LocalRowId::Standalone(row.key),
        }
    }
}

/// Restart initialization prefers the first local CHECKOUT even when a
/// standalone row sorts above the repository group: the documented contract
/// is "first available local checkout", with repository parents and
/// standalone sessions reachable by explicit navigation. Display order still
/// decides among the remaining rows when no checkout exists at all.
pub fn initial_selection(rows: &[LocalRow], remote_ids: &[u64]) -> Option<SelectionTarget> {
    let selectable = selectable_local_ids(rows);
    selectable
        .iter()
        .find(|id| matches!(id, LocalRowId::Checkout(_)))
        .or_else(|| selectable.first())
        .copied()
        .map(SelectionTarget::Local)
        .or_else(|| remote_ids.first().copied().map(SelectionTarget::Remote))
}

/// Sidebar navigation prefers concrete checkouts. A repository parent remains
/// reachable only when it has no available checkout to act on instead.
pub fn selectable_local_ids(rows: &[LocalRow]) -> Vec<LocalRowId> {
    let repositories_with_available_checkout: HashSet<_> = rows
        .iter()
        .filter_map(|row| match row {
            LocalRow::Checkout(checkout)
                if matches!(checkout.health, CheckoutHealth::Available) =>
            {
                Some(checkout.repository_key)
            }
            _ => None,
        })
        .collect();

    rows.iter()
        .filter_map(|row| match row {
            LocalRow::Repository(repository)
                if !repositories_with_available_checkout.contains(&repository.key) =>
            {
                Some(LocalRowId::Repository(repository.key))
            }
            LocalRow::Repository(_) => None,
            LocalRow::Checkout(checkout) => Some(LocalRowId::Checkout(checkout.key)),
            LocalRow::Standalone(standalone) => Some(LocalRowId::Standalone(standalone.key)),
        })
        .collect()
}

pub fn reconcile_selection(
    selected: Option<SelectionTarget>,
    rows: &[LocalRow],
    remote_ids: &[u64],
) -> Option<SelectionTarget> {
    let selectable = selectable_local_ids(rows);
    match selected {
        Some(SelectionTarget::Local(id)) if selectable.contains(&id) => {
            Some(SelectionTarget::Local(id))
        }
        Some(SelectionTarget::Remote(id)) if remote_ids.contains(&id) => {
            Some(SelectionTarget::Remote(id))
        }
        _ => initial_selection(rows, remote_ids),
    }
}

pub fn reconcile_after_removal(
    selected: SelectionTarget,
    before: &[LocalRow],
    after: &[LocalRow],
) -> Option<SelectionTarget> {
    if let SelectionTarget::Local(id) = selected {
        if after.iter().any(|row| row.id() == id) {
            return Some(selected);
        }
    }
    let SelectionTarget::Local(LocalRowId::Checkout(removed)) = selected else {
        return Some(selected);
    };
    let (removed_index, repository_key) =
        before
            .iter()
            .enumerate()
            .find_map(|(index, row)| match row {
                LocalRow::Checkout(child) if child.key == removed => {
                    Some((index, child.repository_key))
                }
                _ => None,
            })?;
    let available: HashSet<_> = after.iter().map(LocalRow::id).collect();
    let sibling = before
        .iter()
        .skip(removed_index + 1)
        .find_map(|row| match row {
            LocalRow::Checkout(child)
                if child.repository_key == repository_key
                    && available.contains(&LocalRowId::Checkout(child.key)) =>
            {
                Some(LocalRowId::Checkout(child.key))
            }
            _ => None,
        })
        .or_else(|| {
            before[..removed_index]
                .iter()
                .rev()
                .find_map(|row| match row {
                    LocalRow::Checkout(child)
                        if child.repository_key == repository_key
                            && available.contains(&LocalRowId::Checkout(child.key)) =>
                    {
                        Some(LocalRowId::Checkout(child.key))
                    }
                    _ => None,
                })
        });
    sibling
        .or_else(|| {
            available
                .contains(&LocalRowId::Repository(repository_key))
                .then_some(LocalRowId::Repository(repository_key))
        })
        .map(SelectionTarget::Local)
}

fn basename(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn suffix_components(path: &Path) -> Vec<String> {
    let mut components: Vec<_> = path
        .components()
        .filter_map(|component| {
            let value = component.as_os_str().to_string_lossy();
            (!value.is_empty() && value != "/").then(|| value.into_owned())
        })
        .collect();
    components.pop();
    components
}

fn display_names(
    parents: &[(RepositoryKey, String, std::path::PathBuf)],
) -> HashMap<RepositoryKey, String> {
    let mut result = HashMap::new();
    let mut groups: HashMap<String, Vec<_>> = HashMap::new();
    for (key, name, path) in parents {
        groups
            .entry(name.to_lowercase())
            .or_default()
            .push((*key, name, path));
    }
    for group in groups.values() {
        if group.len() == 1 {
            let (key, name, _) = group[0];
            result.insert(key, name.clone());
            continue;
        }
        let components: Vec<_> = group
            .iter()
            .map(|(_, _, path)| suffix_components(path))
            .collect();
        for (index, (key, name, _)) in group.iter().enumerate() {
            let own = &components[index];
            let mut suffix = String::new();
            for count in 1..=own.len().max(1) {
                let candidate = own[own.len().saturating_sub(count)..].join("/");
                if components.iter().enumerate().all(|(other_index, other)| {
                    other_index == index
                        || other[other.len().saturating_sub(count)..].join("/") != candidate
                }) {
                    suffix = candidate;
                    break;
                }
            }
            if suffix.is_empty() {
                suffix = format!("#{}", key.get());
            }
            result.insert(*key, format!("{name} ({suffix})"));
        }
    }
    result
}

#[cfg(test)]
pub fn project_local(
    state: &RepositoryState,
    decorations: &HashMap<CheckoutKey, CheckoutDecoration>,
) -> Vec<LocalRow> {
    project_local_with_standalones(state, decorations, &HashMap::new())
}

pub fn project_local_with_standalones(
    state: &RepositoryState,
    decorations: &HashMap<CheckoutKey, CheckoutDecoration>,
    standalone_decorations: &HashMap<StandaloneKey, CheckoutDecoration>,
) -> Vec<LocalRow> {
    let mut repositories: Vec<_> = state.repositories.iter().collect();
    repositories.sort_by(|left, right| {
        let left_path = left.observed_main_worktree.to_path_buf();
        let right_path = right.observed_main_worktree.to_path_buf();
        basename(&left_path)
            .to_lowercase()
            .cmp(&basename(&right_path).to_lowercase())
            .then_with(|| {
                left.observed_main_worktree
                    .as_bytes()
                    .cmp(right.observed_main_worktree.as_bytes())
            })
            .then_with(|| left.key.cmp(&right.key))
    });
    let parent_facts: Vec<_> = repositories
        .iter()
        .map(|repository| {
            let path = repository.observed_main_worktree.to_path_buf();
            (repository.key, basename(&path), path)
        })
        .collect();
    let labels = display_names(&parent_facts);
    let known_repositories: HashSet<_> = repositories
        .iter()
        .map(|repository| repository.key)
        .collect();
    let mut result = Vec::with_capacity(
        state.repositories.len() + state.checkouts.len() + state.standalone_sessions.len(),
    );

    for repository in repositories {
        let mut children: Vec<_> = state
            .checkouts
            .iter()
            .filter(|checkout| checkout.repository_key == repository.key)
            .collect();
        children.sort_by_key(|checkout| (checkout.first_seen_order, checkout.key));
        let waiting_count = children
            .iter()
            .filter(|checkout| {
                decorations.get(&checkout.key).is_some_and(|decoration| {
                    !decoration.archived && decoration.status == Some(Status::Waiting)
                })
            })
            .count();
        let main_path = repository.observed_main_worktree.to_path_buf();
        let name = basename(&main_path);
        let default_capability = children
            .iter()
            .copied()
            .find(|checkout| checkout.role == CheckoutRole::PrimaryDefault)
            .or_else(|| {
                children
                    .iter()
                    .copied()
                    .find(|checkout| checkout.role == CheckoutRole::Main)
            })
            .and_then(|checkout| lifecycle_capability(checkout.lifecycle()));
        result.push(LocalRow::Repository(LocalRepositoryRow {
            key: repository.key,
            display_name: labels
                .get(&repository.key)
                .cloned()
                .unwrap_or_else(|| name.clone()),
            name,
            main_path,
            health: repository.health.clone(),
            child_count: children.len(),
            waiting_count,
            actions: action_view(
                ActionSelection::Repository {
                    available: matches!(repository.health, RepositoryHealth::Available),
                },
                false,
                default_capability,
            ),
        }));
        for checkout in children {
            let decoration = decorations.get(&checkout.key).copied();
            let archived = decoration
                .map(|value| value.archived)
                .unwrap_or(checkout.session.archived);
            let status = if !matches!(checkout.health(), CheckoutHealth::Available) {
                LocalStatus::Unavailable
            } else if archived {
                LocalStatus::Archived
            } else {
                match decoration.and_then(|value| value.status) {
                    Some(Status::Waiting) => LocalStatus::Waiting,
                    Some(Status::Busy) => LocalStatus::Working,
                    Some(Status::Completed) => LocalStatus::Completed,
                    Some(Status::Exited) => LocalStatus::Exited,
                    None => LocalStatus::Closed,
                }
            };
            let capability = lifecycle_capability(checkout.lifecycle());
            let has_runtime = decoration.and_then(|value| value.runtime_id).is_some();
            result.push(LocalRow::Checkout(LocalCheckoutRow {
                key: checkout.key,
                repository_key: checkout.repository_key,
                role: checkout.role,
                managed_by_baude: checkout.managed_by_baude,
                name: checkout.session.name.clone(),
                branch: checkout.session.branch.clone(),
                runtime_id: decoration.and_then(|value| value.runtime_id),
                status,
                waiting_for_ms: decoration.map(|value| value.waiting_for_ms).unwrap_or(0),
                archived,
                health: checkout.health().clone(),
                actions: action_view(
                    ActionSelection::Checkout {
                        role: checkout.role,
                        managed_by_baude: checkout.managed_by_baude,
                        available: matches!(checkout.health(), CheckoutHealth::Available),
                    },
                    has_runtime,
                    capability,
                ),
            }));
        }
    }

    debug_assert!(state
        .checkouts
        .iter()
        .all(|checkout| known_repositories.contains(&checkout.repository_key)));

    for standalone in &state.standalone_sessions {
        let decoration = standalone_decorations.get(&standalone.key).copied();
        let archived = decoration
            .map(|value| value.archived)
            .unwrap_or(standalone.session.archived);
        let available = !matches!(
            standalone.lifecycle(),
            StandaloneLifecycle::Missing
                | StandaloneLifecycle::Io(_)
                | StandaloneLifecycle::ProtectedTeardown(_)
                | StandaloneLifecycle::Stopping(_)
        );
        let status = if !available {
            LocalStatus::Unavailable
        } else if archived {
            LocalStatus::Archived
        } else {
            match decoration.and_then(|value| value.status) {
                Some(Status::Waiting) => LocalStatus::Waiting,
                Some(Status::Busy) => LocalStatus::Working,
                Some(Status::Completed) => LocalStatus::Completed,
                Some(Status::Exited) => LocalStatus::Exited,
                None => LocalStatus::Closed,
            }
        };
        let path = standalone.canonical_path.to_path_buf();
        result.push(LocalRow::Standalone(LocalStandaloneRow {
            key: standalone.key,
            name: standalone.session.name.clone(),
            path,
            runtime_id: decoration.and_then(|value| value.runtime_id),
            status,
            waiting_for_ms: decoration.map(|value| value.waiting_for_ms).unwrap_or(0),
            archived,
            lifecycle: standalone.lifecycle().clone(),
            actions: action_view(
                // Missing folders remain actionable so enter can recheck the
                // canonical path and either reopen or durably retain Missing.
                ActionSelection::Standalone { available: true },
                decoration.and_then(|value| value.runtime_id).is_some(),
                None,
            ),
        }));
    }
    result.sort_by(|left, right| {
        let top = |row: &LocalRow| match row {
            LocalRow::Repository(row) => row.main_path.clone(),
            LocalRow::Checkout(row) => state
                .repositories
                .iter()
                .find(|repository| repository.key == row.repository_key)
                .map(|repository| repository.observed_main_worktree.to_path_buf())
                .unwrap_or_default(),
            LocalRow::Standalone(row) => row.path.clone(),
        };
        let left_path = top(left);
        let right_path = top(right);
        basename(&left_path)
            .to_lowercase()
            .cmp(&basename(&right_path).to_lowercase())
            .then_with(|| left_path.cmp(&right_path))
        // Stable sort preserves each repository's pre-projected parent and
        // first-seen child order when their top-level path is identical.
    });
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use baude_core::lifecycle::LifecycleCapability;
    use baude_core::repository::{
        CheckoutHealth, CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryHealth,
        RepositoryState, RetainedSessionState, RetainedStandaloneSessionState, SavedCheckout,
        SavedRepository, SavedStandaloneSession, StandaloneLifecycle,
    };
    use baude_core::session::Status;

    use super::{
        action_view, initial_selection, project_local, reconcile_after_removal,
        reconcile_selection, selectable_local_ids, ActionKind, ActionSelection, CheckoutDecoration,
        LocalRow, LocalRowId, SelectionTarget,
    };

    fn path(value: &str) -> PersistedPath {
        PersistedPath::from_path(Path::new(value))
    }

    fn add_repository(
        state: &mut RepositoryState,
        main: &str,
    ) -> baude_core::repository::RepositoryKey {
        let key = state.allocate_repository_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        state.repositories.push(SavedRepository {
            key,
            observed_common_dir: path(&format!("{main}/.git")),
            observed_main_worktree: path(main),
            first_seen_order: order,
            health: RepositoryHealth::Available,
        });
        key
    }

    #[allow(clippy::too_many_arguments)]
    fn add_checkout(
        state: &mut RepositoryState,
        repository_key: baude_core::repository::RepositoryKey,
        repository_path: &str,
        checkout_path: &str,
        role: CheckoutRole,
        managed_by_baude: bool,
        first_seen_order: u64,
        branch: &str,
    ) -> baude_core::repository::CheckoutKey {
        let key = state.allocate_checkout_key().unwrap();
        state.next_first_seen_order = state.next_first_seen_order.max(first_seen_order + 1);
        state.checkouts.push(SavedCheckout::new(
            key,
            repository_key,
            role,
            managed_by_baude,
            path(checkout_path),
            Some(format!("refs/heads/{branch}")),
            first_seen_order,
            CheckoutLifecycle::Inactive,
            RetainedSessionState {
                name: format!(
                    "{}:{branch}",
                    Path::new(repository_path)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                ),
                cwd: path(checkout_path),
                repo_root: path(repository_path),
                branch: Some(branch.into()),
                is_worktree: checkout_path != repository_path,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        key
    }

    #[test]
    fn local_hierarchy_orders_parents_and_children_by_durable_identity() {
        let mut state = RepositoryState::default();

        // Admission order intentionally opposes rendered name order.
        let zeta = add_repository(&mut state, "/work/Zeta");
        let project_b = add_repository(&mut state, "/srv/team/project");
        let project_a = add_repository(&mut state, "/home/team/project");

        let newest = add_checkout(
            &mut state,
            project_a,
            "/home/team/project",
            "/worktrees/project-feature",
            CheckoutRole::ManagedBranch,
            true,
            90,
            "feature/newest",
        );
        let main = add_checkout(
            &mut state,
            project_a,
            "/home/team/project",
            "/home/team/project",
            CheckoutRole::Main,
            false,
            20,
            "develop",
        );
        let default = add_checkout(
            &mut state,
            project_a,
            "/home/team/project",
            "/worktrees/project-default",
            CheckoutRole::PrimaryDefault,
            true,
            30,
            "main",
        );
        let oldest = add_checkout(
            &mut state,
            project_a,
            "/home/team/project",
            "/worktrees/project-oldest",
            CheckoutRole::ManagedBranch,
            true,
            10,
            "feature/oldest",
        );

        let decorations = HashMap::from([(
            newest,
            CheckoutDecoration {
                runtime_id: Some(77),
                status: Some(Status::Busy),
                waiting_for_ms: 0,
                archived: false,
            },
        )]);
        let rows = project_local(&state, &decorations);
        let ids: Vec<_> = rows.iter().map(LocalRow::id).collect();

        assert_eq!(
            ids,
            vec![
                LocalRowId::Repository(project_a),
                LocalRowId::Checkout(oldest),
                LocalRowId::Checkout(main),
                LocalRowId::Checkout(default),
                LocalRowId::Checkout(newest),
                LocalRowId::Repository(project_b),
                LocalRowId::Repository(zeta),
            ]
        );
        assert!(rows.iter().any(|row| matches!(
            row,
            LocalRow::Repository(parent) if parent.key == project_b && parent.child_count == 0
        )));
        assert!(rows.iter().any(|row| matches!(
            row,
            LocalRow::Checkout(child)
                if child.key == main && child.runtime_id.is_none() && child.role == CheckoutRole::Main
        )));
        assert!(rows.iter().any(|row| matches!(
            row,
            LocalRow::Checkout(child)
                if child.key == default
                    && child.runtime_id.is_none()
                    && child.role == CheckoutRole::PrimaryDefault
                    && child.actions.kind == ActionKind::Managed
                    && child.actions.capability == Some(LifecycleCapability::RetryReopen)
                    && child.actions.can_remove
        )));
        assert!(rows.iter().any(|row| matches!(
            row,
            LocalRow::Repository(parent)
                if parent.key == project_a
                    && parent.actions.kind == ActionKind::Repository
                    && parent.actions.capability == Some(LifecycleCapability::RetryReopen)
        )));
        assert_eq!(
            action_view(ActionSelection::Remote, true, None).kind,
            ActionKind::Remote
        );

        let duplicate_labels: Vec<_> = rows
            .iter()
            .filter_map(|row| match row {
                LocalRow::Repository(parent) if parent.name == "project" => {
                    Some(parent.display_name.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(duplicate_labels.len(), 2);
        assert_ne!(duplicate_labels[0], duplicate_labels[1]);
        assert!(duplicate_labels
            .iter()
            .all(|label| label.starts_with("project ")));

        // Identical presentation facts still end in a durable-key tie-break.
        let mut tied = RepositoryState::default();
        let first = add_repository(&mut tied, "/same/repo");
        let second = add_repository(&mut tied, "/same/repo");
        let tied_ids: Vec<_> = project_local(&tied, &HashMap::new())
            .iter()
            .map(LocalRow::id)
            .collect();
        assert_eq!(
            tied_ids,
            vec![
                LocalRowId::Repository(first),
                LocalRowId::Repository(second)
            ]
        );
    }

    #[test]
    fn local_hierarchy_order_ignores_runtime_and_session_status() {
        let mut state = RepositoryState::default();
        let repository = add_repository(&mut state, "/repos/steady");
        let first = add_checkout(
            &mut state,
            repository,
            "/repos/steady",
            "/repos/steady",
            CheckoutRole::Main,
            false,
            10,
            "main",
        );
        let second = add_checkout(
            &mut state,
            repository,
            "/repos/steady",
            "/worktrees/steady-second",
            CheckoutRole::ManagedBranch,
            true,
            20,
            "feature/second",
        );
        let expected = vec![
            LocalRowId::Repository(repository),
            LocalRowId::Checkout(first),
            LocalRowId::Checkout(second),
        ];

        for (status, archived) in [
            (Some(Status::Waiting), false),
            (Some(Status::Busy), false),
            (Some(Status::Completed), false),
            (Some(Status::Exited), false),
            (None, false),
            (Some(Status::Waiting), true),
        ] {
            let decorations = HashMap::from([(
                second,
                CheckoutDecoration {
                    runtime_id: status.map(|_| 99),
                    status,
                    waiting_for_ms: 44,
                    archived,
                },
            )]);
            let rows = project_local(&state, &decorations);
            assert_eq!(rows.iter().map(LocalRow::id).collect::<Vec<_>>(), expected);
            assert!(rows.iter().any(|row| matches!(
                row,
                LocalRow::Checkout(child)
                    if child.key == second
                        && child.runtime_id == status.map(|_| 99)
                        && child.archived == archived
            )));
        }

        // Display/session/topology decoration changes cannot become sort keys.
        state.checkouts[1].session.name = "aaa renamed".into();
        state.checkouts[1].session.branch = Some("aaa/renamed".into());
        state.checkouts[1].observed_branch = Some("refs/heads/aaa/renamed".into());
        state.checkouts[1].role = CheckoutRole::PrimaryDefault;
        state.checkouts[1].session.archived = true;
        let saved = state.checkouts[1].clone();
        state.checkouts[1] = SavedCheckout::new(
            saved.key,
            saved.repository_key,
            saved.role,
            saved.managed_by_baude,
            saved.observed_path,
            saved.observed_branch,
            saved.first_seen_order,
            CheckoutLifecycle::Protected(baude_core::repository::UnavailableCause::Missing),
            saved.session,
        );
        assert_eq!(
            project_local(&state, &HashMap::new())
                .iter()
                .map(LocalRow::id)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(project_local(&state, &HashMap::new()).iter().any(|row| {
            matches!(
                row,
                LocalRow::Checkout(child)
                    if child.key == second
                        && child.actions.kind == ActionKind::Unavailable
                        && child.actions.capability.is_none()
            )
        }));
    }

    #[test]
    fn local_hierarchy_selection_survives_refresh_and_removal_falls_back_locally() {
        let mut state = RepositoryState::default();
        let alpha = add_repository(&mut state, "/repos/alpha");
        let alpha_first = add_checkout(
            &mut state,
            alpha,
            "/repos/alpha",
            "/repos/alpha",
            CheckoutRole::Main,
            false,
            10,
            "main",
        );
        let alpha_middle = add_checkout(
            &mut state,
            alpha,
            "/repos/alpha",
            "/worktrees/alpha-middle",
            CheckoutRole::ManagedBranch,
            true,
            20,
            "middle",
        );
        let alpha_last = add_checkout(
            &mut state,
            alpha,
            "/repos/alpha",
            "/worktrees/alpha-last",
            CheckoutRole::ManagedBranch,
            true,
            30,
            "last",
        );
        let beta = add_repository(&mut state, "/repos/beta");
        let beta_child = add_checkout(
            &mut state,
            beta,
            "/repos/beta",
            "/repos/beta",
            CheckoutRole::Main,
            false,
            40,
            "main",
        );
        let before = project_local(&state, &HashMap::new());
        let selected = SelectionTarget::Local(LocalRowId::Checkout(alpha_middle));

        let refreshed = project_local(
            &state,
            &HashMap::from([(
                alpha_middle,
                CheckoutDecoration {
                    runtime_id: Some(700),
                    status: Some(Status::Waiting),
                    waiting_for_ms: 5_000,
                    archived: true,
                },
            )]),
        );
        assert_eq!(
            reconcile_selection(Some(selected), &refreshed, &[81, 82]),
            Some(selected)
        );

        let without_middle: Vec<_> = before
            .iter()
            .filter(|row| row.id() != LocalRowId::Checkout(alpha_middle))
            .cloned()
            .collect();
        assert_eq!(
            reconcile_after_removal(selected, &before, &without_middle),
            Some(SelectionTarget::Local(LocalRowId::Checkout(alpha_last)))
        );

        let without_middle_or_last: Vec<_> = before
            .iter()
            .filter(|row| {
                !matches!(
                    row.id(),
                    LocalRowId::Checkout(key) if key == alpha_middle || key == alpha_last
                )
            })
            .cloned()
            .collect();
        assert_eq!(
            reconcile_after_removal(selected, &before, &without_middle_or_last),
            Some(SelectionTarget::Local(LocalRowId::Checkout(alpha_first)))
        );

        let without_alpha_children: Vec<_> = before
            .iter()
            .filter(|row| {
                !matches!(
                    row,
                    LocalRow::Checkout(child) if child.repository_key == alpha
                )
            })
            .cloned()
            .collect();
        assert_eq!(
            reconcile_after_removal(selected, &before, &without_alpha_children),
            Some(SelectionTarget::Local(LocalRowId::Repository(alpha)))
        );
        assert!(without_alpha_children
            .iter()
            .any(|row| { matches!(row.id(), LocalRowId::Checkout(key) if key == beta_child) }));

        assert_eq!(
            initial_selection(&before, &[81, 82]),
            Some(SelectionTarget::Local(LocalRowId::Checkout(alpha_first)))
        );
        assert_eq!(
            selectable_local_ids(&before),
            vec![
                LocalRowId::Checkout(alpha_first),
                LocalRowId::Checkout(alpha_middle),
                LocalRowId::Checkout(alpha_last),
                LocalRowId::Checkout(beta_child),
            ]
        );
        assert_eq!(
            reconcile_selection(
                Some(SelectionTarget::Local(LocalRowId::Repository(alpha))),
                &before,
                &[81, 82],
            ),
            Some(SelectionTarget::Local(LocalRowId::Checkout(alpha_first)))
        );

        let unavailable_only: Vec<_> = project_local(&state, &HashMap::new())
            .into_iter()
            .filter(|row| match row {
                LocalRow::Repository(parent) => parent.key == alpha,
                LocalRow::Checkout(child) => child.repository_key == alpha,
                LocalRow::Standalone(_) => false,
            })
            .map(|row| match row {
                LocalRow::Repository(parent) => LocalRow::Repository(parent),
                LocalRow::Checkout(mut child) => {
                    child.health = CheckoutHealth::Unavailable(
                        baude_core::repository::UnavailableCause::Missing,
                    );
                    LocalRow::Checkout(child)
                }
                LocalRow::Standalone(standalone) => LocalRow::Standalone(standalone),
            })
            .collect();
        assert_eq!(
            selectable_local_ids(&unavailable_only).first().copied(),
            Some(LocalRowId::Repository(alpha))
        );
        assert_eq!(
            initial_selection(&[], &[81, 82]),
            Some(SelectionTarget::Remote(81))
        );
        assert_eq!(initial_selection(&[], &[]), None);
    }

    #[test]
    fn standalone_rows_are_deterministic_top_level_sessions_without_git_authority() {
        let mut state = RepositoryState::default();
        let repository = add_repository(&mut state, "/work/beta");
        let checkout = add_checkout(
            &mut state,
            repository,
            "/work/beta",
            "/work/beta",
            CheckoutRole::Main,
            false,
            20,
            "main",
        );
        let key = state.allocate_standalone_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        state.standalone_sessions.push(SavedStandaloneSession::new(
            key,
            path("/work/alpha"),
            order,
            StandaloneLifecycle::Inactive,
            None,
            RetainedStandaloneSessionState {
                name: "alpha".into(),
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: Some("resume-alpha".into()),
                ever_launched: true,
            },
        ));

        let rows = super::project_local_with_standalones(&state, &HashMap::new(), &HashMap::new());
        assert_eq!(rows[0].id(), LocalRowId::Standalone(key));
        let LocalRow::Standalone(row) = &rows[0] else {
            panic!("expected root-level standalone row");
        };
        assert_eq!(row.path, Path::new("/work/alpha"));
        assert_eq!(row.actions.kind, ActionKind::Standalone);
        assert!(!row.actions.can_activate_branch);
        assert!(!row.actions.can_remove);
        assert_eq!(selectable_local_ids(&rows)[0], LocalRowId::Standalone(key));

        // The standalone row sorts and navigates first, but restart
        // initialization still lands on the first checkout — the documented
        // "first available local checkout" contract. Only when no checkout
        // row exists at all does display order decide.
        assert_eq!(
            initial_selection(&rows, &[81]),
            Some(SelectionTarget::Local(LocalRowId::Checkout(checkout)))
        );
        let without_checkouts: Vec<_> = rows
            .iter()
            .filter(|row| !matches!(row, LocalRow::Checkout(_)))
            .cloned()
            .collect();
        assert_eq!(
            initial_selection(&without_checkouts, &[81]),
            Some(SelectionTarget::Local(LocalRowId::Standalone(key)))
        );
    }
}
