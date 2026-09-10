use git2::Repository;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Split a `"Name <email>"` author string into its parts, falling back to a
/// safe default if it isn't in that shape. Used only when git has no
/// `user.name` / `user.email` configured (see `Settings::git_signature_fallback`).
fn parse_author(raw: &str) -> (String, String) {
    if let Some((name, rest)) = raw.split_once('<') {
        let name = name.trim();
        let email = rest.trim_end_matches('>').trim();
        if !name.is_empty() && !email.is_empty() {
            return (name.to_string(), email.to_string());
        }
    }
    ("Rhymr".to_string(), "rhymr@local".to_string())
}

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

/// Where a history walk starts, for [`GitController::log`].
pub enum LogStart {
    /// The current `HEAD`.
    Head,
    /// A named branch or revision (anything `git rev-parse` accepts).
    Branch(String),
    /// Every ref — local + remote branches and tags (the "all branches" view).
    AllRefs,
}

/// One row of the Git Log list: enough to render it and draw the graph edges.
#[derive(Clone, Debug)]
pub struct CommitSummary {
    pub id: git2::Oid,
    /// Abbreviated hash git considers unambiguous (usually 7 chars).
    pub short_id: String,
    /// First line of the message.
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    /// Parents, first-parent first — one entry for a normal commit, two+ for a
    /// merge, none for the root.
    pub parent_ids: Vec<git2::Oid>,
}

/// A path touched by a commit, with its coarse change kind.
#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub status: git2::Delta,
}

/// Everything the Git Log detail pane shows for the selected commit.
#[derive(Clone, Debug)]
pub struct CommitDetail {
    pub summary: CommitSummary,
    /// Full commit message (subject + body).
    pub body: String,
    pub committer_name: String,
    pub committer_email: String,
    /// Commit time, seconds since the Unix epoch.
    pub commit_time: i64,
    /// Files changed vs the first parent (vs the empty tree for a root commit),
    /// sorted by path.
    pub files: Vec<ChangedFile>,
}

/// Kind of ref pointing at a commit, for the coloured chips in the log.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

/// A ref label to chip onto a commit row in the log.
#[derive(Clone, Debug)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

/// Build a [`CommitSummary`] from a libgit2 commit.
fn summarize_commit(commit: &git2::Commit<'_>) -> CommitSummary {
    let author = commit.author();
    let id = commit.id();
    let short_id = commit
        .as_object()
        .short_id()
        .ok()
        .and_then(|buf| buf.as_str().ok().map(str::to_string))
        .unwrap_or_else(|| {
            let hex = id.to_string();
            hex[..hex.len().min(7)].to_string()
        });
    CommitSummary {
        id,
        short_id,
        summary: commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string(),
        author_name: author.name().unwrap_or_default().to_string(),
        author_email: author.email().unwrap_or_default().to_string(),
        time: commit.time().seconds(),
        parent_ids: commit.parent_ids().collect(),
    }
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
        Err(e) => {
            log::trace!("line_changes: diff for {rel:?} failed: {e}");
            return out;
        }
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
        log::info!("git commit in {:?}", self.repo_path);
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let mut index = repo.index().map_err(|e| e.to_string())?;

        // Add all plain text files to stage
        index
            .add_all(["*.txt"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| e.to_string())?;
        index.write().map_err(|e| e.to_string())?;

        let tree_id = index.write_tree().map_err(|e| e.to_string())?;
        let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;

        let signature = match repo.signature() {
            Ok(sig) => sig,
            Err(_) => {
                let (name, email) =
                    parse_author(&crate::setting::Settings::load().git_signature_fallback);
                git2::Signature::now(&name, &email).map_err(|e| e.to_string())?
            }
        };

        let parent_commit = match repo.head() {
            Ok(head) => Some(head.peel_to_commit().map_err(|e| e.to_string())?),
            Err(_) => None,
        };

        let parents = match &parent_commit {
            Some(c) => vec![c],
            None => vec![],
        };

        let oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .map_err(|e| {
                log::warn!("git commit failed: {e}");
                e.to_string()
            })?;
        log::info!("git commit {oid}");
        Ok(oid)
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
        log::info!("git fetch from {remote_name}");
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
        log::info!("git pull from {remote_name}");
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
            log::warn!(
                "git pull: '{branch_name}' has diverged from {remote_name} — not fast-forwardable"
            );
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
        log::info!("git push to {remote_name}");
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
            .map_err(|e| {
                log::warn!("git push failed: {e}");
                e.to_string()
            })?;

        log::info!("git push: '{branch_name}' → {remote_name}");
        Ok(format!("Pushed '{branch_name}' to {remote_name}."))
    }

    /// Walk history from `start`, newest first, skipping `skip` commits and
    /// returning at most `limit`. Paged so the Git Log panel never loads the
    /// whole history at once. An unborn branch (no commits yet) yields an
    /// empty vec rather than an error.
    pub fn log(
        &self,
        start: LogStart,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<CommitSummary>, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
        walk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .map_err(|e| e.to_string())?;

        match start {
            LogStart::Head => {
                if repo.head().is_err() {
                    return Ok(Vec::new());
                }
                walk.push_head().map_err(|e| e.to_string())?;
            }
            LogStart::Branch(rev) => {
                let obj = repo.revparse_single(&rev).map_err(|e| e.to_string())?;
                let commit_id = obj
                    .peel_to_commit()
                    .map(|c| c.id())
                    .unwrap_or_else(|_| obj.id());
                walk.push(commit_id).map_err(|e| e.to_string())?;
            }
            LogStart::AllRefs => {
                if walk.push_glob("refs/heads/*").is_err() && repo.head().is_ok() {
                    walk.push_head().map_err(|e| e.to_string())?;
                }
                let _ = walk.push_glob("refs/remotes/*");
                let _ = walk.push_glob("refs/tags/*");
            }
        }

        let mut out = Vec::with_capacity(limit.min(1024));
        for oid in walk.skip(skip).take(limit) {
            let oid = oid.map_err(|e| e.to_string())?;
            let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
            out.push(summarize_commit(&commit));
        }
        Ok(out)
    }

    /// Full metadata + changed-file list for one commit. The file list is the
    /// diff of the commit against its first parent (against the empty tree for
    /// a root commit).
    pub fn commit_detail(&self, id: git2::Oid) -> Result<CommitDetail, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let commit = repo.find_commit(id).map_err(|e| e.to_string())?;

        let tree = commit.tree().map_err(|e| e.to_string())?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree().map_err(|e| e.to_string())?),
            Err(_) => None,
        };
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
            .map_err(|e| e.to_string())?;

        let mut files: Vec<ChangedFile> = diff
            .deltas()
            .map(|delta| {
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
                ChangedFile {
                    path,
                    status: delta.status(),
                }
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let committer = commit.committer();
        Ok(CommitDetail {
            body: commit.message().unwrap_or_default().to_string(),
            committer_name: committer.name().unwrap_or_default().to_string(),
            committer_email: committer.email().unwrap_or_default().to_string(),
            commit_time: commit.time().seconds(),
            files,
            summary: summarize_commit(&commit),
        })
    }

    /// Every ref (local + remote branches, tags, and `HEAD`) grouped by the
    /// commit it ultimately points at, for the chips shown on log rows.
    pub fn ref_labels(&self) -> Result<HashMap<git2::Oid, Vec<RefLabel>>, String> {
        let repo = Repository::open(&self.repo_path).map_err(|e| e.to_string())?;
        let mut out: HashMap<git2::Oid, Vec<RefLabel>> = HashMap::new();

        let refs = repo.references().map_err(|e| e.to_string())?;
        for reference in refs.flatten() {
            let Some(target) = reference.target() else {
                continue;
            };
            // Peel annotated tags to the commit they wrap.
            let oid = repo
                .find_object(target, None)
                .and_then(|obj| obj.peel_to_commit())
                .map(|commit| commit.id())
                .unwrap_or(target);

            let kind = if reference.is_branch() {
                RefKind::LocalBranch
            } else if reference.is_remote() {
                RefKind::RemoteBranch
            } else if reference.is_tag() {
                RefKind::Tag
            } else {
                continue;
            };
            let name = reference.shorthand().unwrap_or_default().to_string();
            if !name.is_empty() {
                out.entry(oid).or_default().push(RefLabel { name, kind });
            }
        }

        if let Ok(head) = repo.head()
            && let Ok(commit) = head.peel_to_commit()
        {
            out.entry(commit.id()).or_default().push(RefLabel {
                name: "HEAD".to_string(),
                kind: RefKind::Head,
            });
        }

        Ok(out)
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
    if let Err(e) = index.write() {
        log::warn!("git autostage: writing the index failed: {e}");
    } else {
        log::debug!("git autostage: staged all changes in {workspace_root:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller pointed at this crate's own checkout — always a git repo
    /// under `cargo test` (a normal clone in CI, a linked worktree locally;
    /// `Repository::open` resolves both).
    fn controller() -> GitController {
        GitController::new(Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    #[test]
    fn log_pages_and_skips() {
        let git = controller();
        let first = git.log(LogStart::Head, 5, 0).expect("log head");
        assert!(!first.is_empty(), "this repo has commits");
        assert!(first.len() <= 5);
        assert!(first.iter().all(|c| !c.short_id.is_empty()));

        if first.len() > 1 {
            let skipped = git.log(LogStart::Head, 5, 1).expect("log skip");
            assert_eq!(
                skipped.first().map(|c| c.id),
                first.get(1).map(|c| c.id),
                "skip=1 drops exactly the newest commit"
            );
        }
    }

    #[test]
    fn commit_detail_matches_its_summary() {
        let git = controller();
        let head = git
            .log(LogStart::Head, 1, 0)
            .expect("log head")
            .pop()
            .expect("at least one commit");
        let detail = git.commit_detail(head.id).expect("commit detail");
        assert_eq!(detail.summary.id, head.id);
        assert_eq!(detail.summary.short_id, head.short_id);
        assert!(detail.body.starts_with(&head.summary) || head.summary.is_empty());
    }

    #[test]
    fn ref_labels_include_a_head_chip() {
        let git = controller();
        let labels = git.ref_labels().expect("ref labels");
        assert!(
            labels.values().flatten().any(|l| l.kind == RefKind::Head),
            "a HEAD chip should point at some commit"
        );
    }
}
