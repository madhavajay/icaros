use anyhow::{Context, Result};
use git2::{DiffOptions, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GitFile {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub staged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl GitFileStatus {
    pub fn to_str(self) -> &'static str {
        match self {
            GitFileStatus::Modified => "M",
            GitFileStatus::Added => "A",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Untracked => "??",
        }
    }

    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            GitFileStatus::Modified => Color::Yellow,
            GitFileStatus::Added => Color::Green,
            GitFileStatus::Deleted => Color::Red,
            GitFileStatus::Renamed => Color::Blue,
            GitFileStatus::Untracked => Color::Gray,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub staged: bool,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub origin: char,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

pub struct GitManager {
    repo: Repository,
}

impl GitManager {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path).context("Failed to open git repository")?;
        Ok(GitManager { repo })
    }

    pub fn get_status_files(&self) -> Result<Vec<GitFile>> {
        let mut files = Vec::new();
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true).include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut status_opts))?;

        for entry in statuses.iter() {
            let status = entry.status();
            let path = entry.path().unwrap_or("");

            // Skip if no status bits are set
            if status.is_empty() {
                continue;
            }

            let file_status = if status.contains(Status::WT_NEW) {
                GitFileStatus::Untracked
            } else if status.contains(Status::WT_DELETED) || status.contains(Status::INDEX_DELETED)
            {
                GitFileStatus::Deleted
            } else if status.contains(Status::WT_RENAMED) || status.contains(Status::INDEX_RENAMED)
            {
                GitFileStatus::Renamed
            } else if status.contains(Status::INDEX_NEW) {
                GitFileStatus::Added
            } else if status.contains(Status::WT_MODIFIED)
                || status.contains(Status::INDEX_MODIFIED)
            {
                GitFileStatus::Modified
            } else {
                continue;
            };

            let staged = status.contains(Status::INDEX_NEW)
                || status.contains(Status::INDEX_MODIFIED)
                || status.contains(Status::INDEX_DELETED)
                || status.contains(Status::INDEX_RENAMED);

            files.push(GitFile {
                path: PathBuf::from(path),
                status: file_status,
                staged,
            });
        }

        Ok(files)
    }

    pub fn get_file_diff(&self, file_path: &Path, staged: bool) -> Result<Vec<GitHunk>> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(file_path);

        let diff = if staged {
            // Show staged changes (index vs HEAD)
            let head = self.repo.head()?.peel_to_tree()?;
            let mut index = self.repo.index()?;
            let oid = index.write_tree()?;
            let index_tree = self.repo.find_tree(oid)?;
            self.repo
                .diff_tree_to_tree(Some(&head), Some(&index_tree), Some(&mut diff_opts))?
        } else {
            // Show unstaged changes (workdir vs index)
            self.repo
                .diff_index_to_workdir(None, Some(&mut diff_opts))?
        };

        let mut hunks = Vec::new();

        // Use RefCell to share mutable state between closures
        use std::cell::RefCell;
        let current_hunk = RefCell::new(None::<GitHunk>);

        diff.foreach(
            &mut |_delta, _| true,
            None,
            Some(&mut |_delta, hunk| {
                let hunk_header = std::str::from_utf8(hunk.header()).unwrap_or("");

                // Start a new hunk
                if let Some(hunk) = current_hunk.borrow_mut().take() {
                    hunks.push(hunk);
                }
                *current_hunk.borrow_mut() = Some(GitHunk {
                    old_start: hunk.old_start(),
                    old_lines: hunk.old_lines(),
                    new_start: hunk.new_start(),
                    new_lines: hunk.new_lines(),
                    header: hunk_header.to_string(),
                    lines: Vec::new(),
                    staged,
                });
                true
            }),
            Some(&mut |_delta, _hunk, line| {
                let content = std::str::from_utf8(line.content())
                    .unwrap_or("")
                    .to_string();
                let diff_line = DiffLine {
                    origin: line.origin(),
                    content,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                };

                if let Some(ref mut hunk) = current_hunk.borrow_mut().as_mut() {
                    hunk.lines.push(diff_line);
                }

                true
            }),
        )?;

        // Don't forget the last hunk
        if let Some(hunk) = current_hunk.into_inner() {
            hunks.push(hunk);
        }

        Ok(hunks)
    }

    pub fn stage_file(&self, file_path: &Path) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_path(file_path)?;
        index.write()?;
        Ok(())
    }

    pub fn unstage_file(&self, file_path: &Path) -> Result<()> {
        let head = self.repo.head()?.peel_to_tree()?;
        let _index = self.repo.index()?;

        // Reset the file in the index to match HEAD
        self.repo
            .reset_default(Some(&head.into_object()), [file_path])?;

        Ok(())
    }

    pub fn stage_hunk(&self, _file_path: &Path, _hunk: &GitHunk) -> Result<()> {
        // This is more complex and would require patching
        // For now, return an error indicating it's not implemented
        anyhow::bail!("Staging individual hunks is not yet implemented")
    }

    pub fn unstage_hunk(&self, _file_path: &Path, _hunk: &GitHunk) -> Result<()> {
        // This is more complex and would require patching
        // For now, return an error indicating it's not implemented
        anyhow::bail!("Unstaging individual hunks is not yet implemented")
    }
}
