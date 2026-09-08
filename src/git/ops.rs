use git2::Repository;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A file's status relative to HEAD, simplified to the categories the file
/// tree colors differently. Checked in this priority order (a renamed file
/// that also changed content still reads as "Renamed", matching `git
/// status`'s own compact summary).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GitFileStatus {
    New,
    Renamed,
    Modified,
}

/// Per-line change status of a file's current text vs the version in HEAD,
/// for the editor's VCS gutter bars. `Deleted` marks the surviving line at
/// the seam where one or more lines were removed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineChange {
    Added,
    Modified,
    Deleted,
}

/// Change type keyed by 0-based line number of `current_text`. Empty when
/// `file_abs` isn't inside `repo_root`'s repo, there's no HEAD, or nothing
/// changed. A file with no blob in HEAD (brand new / untracked) reports
/// every line `Added`. Granularity is per hunk (the whole changed hunk gets
/// one color), matching how JetBrains paints the gutter.
pub fn line_changes(
    repo_root: &Path,
    file_abs: &Path,
    current_text: &str,
) -> HashMap<usize, LineChange> {
    let mut out = HashMap::new();

    let Ok(repo) = Repository::open(repo_root) else {
        return out;
    };
    let Ok(rel) = file_abs.strip_prefix(repo_root) else {
        return out;
    };

    let head_blob = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok())
        .and_then(|tree| tree.get_path(rel).ok())
        .and_then(|entry| repo.find_blob(entry.id()).ok());

    let Some(head_blob) = head_blob else {
        // Untracked / newly added — the whole file is new.
        for line in 0..current_text.lines().count() {
            out.insert(line, LineChange::Added);
        }
        return out;
    };

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0);

    let patch = match git2::Patch::from_blob_and_buffer(
        &head_blob,
        Some(rel),
        current_text.as_bytes(),
        Some(rel),
        Some(&mut opts),
    ) {
        Ok(patch) => patch,
        Err(_) => return out,
    };

    for h in 0..patch.num_hunks() {
        let Ok((hunk, _)) = patch.hunk(h) else {
            continue;
        };
        let new_start = hunk.new_start();
        let new_lines = hunk.new_lines();
        let old_lines = hunk.old_lines();

        if new_lines == 0 {
            // Pure deletion — flag the line just after the removed block.
            out.entry(new_start.saturating_sub(1) as usize)
                .or_insert(LineChange::Deleted);
            continue;
        }

        let kind = if old_lines == 0 {
            LineChange::Added
        } else {
            LineChange::Modified
        };
        let start = new_start.saturating_sub(1) as usize;
        for line in start..start + new_lines as usize {
            out.insert(line, kind);
        }
    }

    out
}

pub struct GitController {
    repo_path: std::path::PathBuf,
}

impl GitController {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Commit all modified/new .txt files with a specific message
    pub fn commit_all(&self, message: &str) -> Result<git2::Oid, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let mut index = repo.index().map_err(|e| e.to_string())?;

        // Add all plain text files to stage
        index
            .add_all(["*.txt"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| e.to_string())?;
        index.write().map_err(|e| e.to_string())?;

        let tree_id = index.write_tree().map_err(|e| e.to_string())?;
        let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;

        let signature = repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("Pneuma", "pneuma@local").unwrap());

        let parent_commit = match repo.head() {
            Ok(head) => Some(head.peel_to_commit().map_err(|e| e.to_string())?),
            Err(_) => None,
        };

        let parents = match &parent_commit {
            Some(c) => vec![c],
            None => vec![],
        };

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .map_err(|e| e.to_string())
    }

    /// Absolute paths of tracked/untracked files that differ from HEAD —
    /// new, renamed, modified, deleted, or type-changed, staged or not —
    /// bucketed into the categories the file tree colors differently.
    /// Ignored files are excluded.
    pub fn file_statuses(&self) -> HashMap<PathBuf, GitFileStatus> {
        let mut result = HashMap::new();

        let Ok(repo) = Repository::open(&self.repo_path) else {
            return result;
        };
        let Ok(statuses) = repo.statuses(None) else {
            return result;
        };

        let new_flags = git2::Status::WT_NEW | git2::Status::INDEX_NEW;
        let renamed_flags = git2::Status::WT_RENAMED | git2::Status::INDEX_RENAMED;
        let modified_flags = git2::Status::WT_MODIFIED
            | git2::Status::WT_DELETED
            | git2::Status::WT_TYPECHANGE
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_TYPECHANGE;

        for entry in statuses.iter() {
            let status = entry.status();
            if status.is_ignored() {
                continue;
            }

            let category = if status.intersects(new_flags) {
                GitFileStatus::New
            } else if status.intersects(renamed_flags) {
                GitFileStatus::Renamed
            } else if status.intersects(modified_flags) {
                GitFileStatus::Modified
            } else {
                continue;
            };

            if let Ok(path) = entry.path() {
                result.insert(self.repo_path.join(path), category);
            }
        }

        result
    }

    /// Absolute paths of entries git ignores (one entry per ignored
    /// directory — the contents aren't enumerated). Empty when the path
    /// isn't a repo. Used to grey ignored rows in the file tree.
    pub fn ignored_paths(&self) -> HashSet<PathBuf> {
        let mut set = HashSet::new();
        let Ok(repo) = Repository::open(&self.repo_path) else {
            return set;
        };
        let mut opts = git2::StatusOptions::new();
        opts.include_ignored(true)
            .recurse_ignored_dirs(false)
            .include_untracked(false);
        let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
            return set;
        };
        for entry in statuses.iter() {
            if entry.status().is_ignored()
                && let Ok(path) = entry.path()
            {
                set.insert(self.repo_path.join(path.trim_end_matches('/')));
            }
        }
        set
    }

    /// The checked-out branch's short name (e.g. `"main"`), or `None` on a
    /// detached HEAD or an unborn/empty repo.
    pub fn current_branch_name(&self) -> Option<String> {
        let repo = Repository::open(&self.repo_path).ok()?;
        let head = repo.head().ok()?;
        head.shorthand().ok().map(str::to_string)
    }

    /// Whether a remote named `name` (e.g. `"origin"`) is configured.
    pub fn has_remote(&self, name: &str) -> bool {
        let Ok(repo) = Repository::open(&self.repo_path) else {
            return false;
        };
        repo.find_remote(name).is_ok()
    }

    /// Fetch `remote_name`, returning a one-line human-readable summary.
    pub fn fetch(&self, remote_name: &str) -> Result<String, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let mut remote = repo.find_remote(remote_name).map_err(|e| e.to_string())?;
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(remote_callbacks());
        remote
            .fetch::<&str>(&[], Some(&mut fetch_opts), None)
            .map_err(|e| e.to_string())?;

        let stats = remote.stats();
        if stats.received_objects() == 0 {
            Ok("Already up to date.".to_string())
        } else {
            Ok(format!(
                "Fetched {} object(s) from {remote_name}.",
                stats.received_objects()
            ))
        }
    }

    /// Fetch `remote_name` and fast-forward the current branch to match —
    /// deliberately doesn't attempt a merge or rebase when the branches have
    /// diverged, since resolving that safely needs a real merge-conflict UI
    /// this app doesn't have; it reports back instead so the user can
    /// resolve it another way (e.g. the terminal).
    pub fn pull(&self, remote_name: &str) -> Result<String, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let branch_name = self
            .current_branch_name()
            .ok_or("Detached HEAD — can't pull".to_string())?;

        let mut remote = repo.find_remote(remote_name).map_err(|e| e.to_string())?;
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(remote_callbacks());
        remote
            .fetch(&[branch_name.as_str()], Some(&mut fetch_opts), None)
            .map_err(|e| e.to_string())?;

        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| e.to_string())?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(|e| e.to_string())?;

        let (analysis, _) = repo
            .merge_analysis(&[&fetch_commit])
            .map_err(|e| e.to_string())?;
        if analysis.is_up_to_date() {
            return Ok("Already up to date.".to_string());
        }
        if !analysis.is_fast_forward() {
            return Err(format!(
                "'{branch_name}' has diverged from {remote_name}/{branch_name} — can't fast-forward. Resolve manually."
            ));
        }

        let refname = format!("refs/heads/{branch_name}");
        let mut reference = repo.find_reference(&refname).map_err(|e| e.to_string())?;
        reference
            .set_target(fetch_commit.id(), "Fast-forward pull")
            .map_err(|e| e.to_string())?;
        repo.set_head(&refname).map_err(|e| e.to_string())?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .map_err(|e| e.to_string())?;

        Ok(format!(
            "Fast-forwarded '{branch_name}' to {remote_name}/{branch_name}."
        ))
    }

    /// Push the current branch to `remote_name`, creating/updating the same
    /// branch name there.
    pub fn push(&self, remote_name: &str) -> Result<String, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let branch_name = self
            .current_branch_name()
            .ok_or("Detached HEAD — can't push".to_string())?;

        let mut remote = repo.find_remote(remote_name).map_err(|e| e.to_string())?;
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(remote_callbacks());

        let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");
        remote
            .push(&[refspec.as_str()], Some(&mut push_opts))
            .map_err(|e| e.to_string())?;

        Ok(format!("Pushed '{branch_name}' to {remote_name}."))
    }
}

/// Credential resolution shared by fetch/pull/push: SSH-agent for `git@`/
/// `ssh://` remotes, falling back to the OS credential helper (e.g.
/// git-credential-osxkeychain) for HTTPS — the same two sources plain `git`
/// itself tries first, so this "just works" for anyone who can already
/// `git push` from the terminal without being prompted.
fn remote_callbacks<'a>() -> git2::RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY)
            && let Some(username) = username_from_url
            && let Ok(cred) = git2::Cred::ssh_key_from_agent(username)
        {
            return Ok(cred);
        }
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            let config = git2::Config::open_default().or_else(|_| git2::Config::new());
            if let Ok(config) = config
                && let Ok(cred) = git2::Cred::credential_helper(&config, url, username_from_url)
            {
                return Ok(cred);
            }
        }
        git2::Cred::default()
    });
    callbacks
}

/// Stage every change in the workspace (new, modified, and deleted files
/// alike — equivalent to `git add -A`), if it's a git repo. A no-op
/// (including on any git error) if it isn't, so callers can fire this after
/// every disk-mutating file operation without checking first.
pub fn stage_all_changes(workspace_root: &Path) {
    if !workspace_root.join(".git").is_dir() {
        return;
    }

    let Ok(repo) = Repository::open(workspace_root) else {
        return;
    };
    let Ok(mut index) = repo.index() else {
        return;
    };

    // add_all() picks up new/modified files; update_all() additionally
    // stages files that were deleted from the working tree.
    let _ = index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None);
    let _ = index.update_all(["*"].iter(), None);
    let _ = index.write();
}
