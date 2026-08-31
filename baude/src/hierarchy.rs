#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use baude_core::repository::{
        CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryState,
        RetainedSessionState, SavedCheckout, SavedRepository,
    };
    use baude_core::session::Status;

    use super::{project_local, CheckoutDecoration, LocalRow, LocalRowId};

    fn path(value: &str) -> PersistedPath {
        PersistedPath::from_path(Path::new(value))
    }

    fn add_repository(state: &mut RepositoryState, main: &str) -> baude_core::repository::RepositoryKey {
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
                name: format!("{}:{branch}", Path::new(repository_path).file_name().unwrap().to_string_lossy()),
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
        )));

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
        assert!(duplicate_labels.iter().all(|label| label.starts_with("project ")));

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
            vec![LocalRowId::Repository(first), LocalRowId::Repository(second)]
        );
    }
}
