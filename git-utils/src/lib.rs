use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use git2::{Branches, BranchType, Commit, Repository, Signature};
use git2::build::RepoBuilder;
use tempdir::TempDir;

pub struct TestGit {
    pub dir: PathBuf,
    pub remote: Repository,
    pub local: Repository,
}

// Initialise a git repo with a remote in the state that
// remote - 3 commits, initial, first commit, second commit
// local - 2 commits, initial, first commit
// So that if we fetch on local then it should know about the second file
pub fn initialise_git_repo(path: Option<&PathBuf>) -> TestGit {
    let tmp_dir = TempDir::new("repo").expect("Temp dir created").into_path();
    let remote_path = tmp_dir.join("remote");
    let local_path = tmp_dir.join("local");
    match path {
        Some(p) => copy_recursively(p, &remote_path),
        None => std::fs::create_dir(&remote_path),
    }.unwrap();
    std::fs::create_dir(&local_path).unwrap();

    let remote = Repository::init(&remote_path).unwrap();
    create_initial_commit(&remote);
    create_file(&remote_path, "new_file");
    git_add_all(&remote);
    git_commit(&remote, "First commit");

    let local = git_clone_local(&remote, &local_path);

    create_file(&remote_path, "new_file2");
    git_add_all(&remote);
    git_commit(&remote, "Second commit");

    git_checkout(&remote, "other", true);
    create_file(&remote_path, "new_file3");
    git_add_all(&remote);
    git_commit(&remote, "Third commit");
    git_checkout(&remote, "main", false);

    TestGit {
        dir: tmp_dir,
        remote,
        local,
    }
}

/// Copy files from source to destination recursively.
/// From https://nick.groenen.me/notes/recursively-copy-files-in-rust/
pub fn copy_recursively(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(&destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn create_file(repo_path: &Path, file_name: &str) {
    let mut file = File::create(repo_path.join(file_name)).unwrap();
    file.write_all(b"File contents").unwrap();
}

fn create_initial_commit(repo: &Repository) {
    let signature = Signature::now("Test User", "test.user@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )
        .unwrap();
}


fn git_add_all(repo: &Repository) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["."], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
}

fn git_commit(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    let oid = index.write_tree().unwrap();
    let signature = Signature::now("Test User", "test.user@example.com").unwrap();
    let parent_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = repo.find_tree(oid).unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent_commit],
    )
        .unwrap();
}

fn git_clone_local(from: &Repository, to: &Path) -> Repository {
    let mut builder = RepoBuilder::new();
    builder.clone(from.path().to_str().unwrap(), to).unwrap()
}

pub fn git_get_latest_commit<'a>(repo: &'a Repository, reference: &str) -> Commit<'a> {
    repo.find_reference(reference)
        .unwrap()
        .resolve()
        .unwrap()
        .peel_to_commit()
        .unwrap()
}

fn git_checkout(repo: &Repository, branch_name: &str, new_branch: bool) {
    if new_branch {
        let head = repo.head().unwrap();
        let oid = head.target().unwrap();
        let commit = repo.find_commit(oid).unwrap();
        repo.branch(branch_name, &commit, false).unwrap();
    }

    let (object, reference) = repo.revparse_ext(branch_name).expect("Branch not found");
    repo.checkout_tree(&object, None).unwrap();
    // Checkout tree only sets contents of working tree, we need to set HEAD too
    // otherwise we leave git ina  dirty state
    repo.set_head(reference.unwrap().name().unwrap()).unwrap();
}

pub fn git_remote_branches(repo: &Repository) -> Branches {
    repo.branches(Some(BranchType::Remote)).unwrap()
}
