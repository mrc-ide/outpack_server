use std::path::Path;

use git2::{Branch, BranchType, Reference, Repository};
use serde::{Deserialize, Serialize};

pub fn git_fetch(root: &Path) -> Result<(), git2::Error> {
    let repo = Repository::open(root)?;
    let mut remote = repo.find_remote("origin")?;
    let ref_specs_iter = remote.fetch_refspecs()?;
    let ref_specs: Vec<&str> = ref_specs_iter.iter().map(|spec| spec.unwrap()).collect();
    remote.fetch(&ref_specs, None, None)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct BranchResponse {
    default_branch: Option<String>,
    branches: Vec<BranchInfo>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BranchInfo {
    name: String,
    commit_hash: String,
    time: i64,
    message: Vec<String>,
}

fn get_branch_name(reference: &Reference) -> String {
    let lossy_name = String::from_utf8_lossy(reference.name_bytes());
    lossy_name
        .strip_prefix("refs/remotes/origin/")
        .unwrap_or_else(|| &lossy_name)
        .to_string()
}

fn get_branch_info(branch: Branch) -> Result<BranchInfo, git2::Error> {
    let git_ref = branch.get().resolve()?;
    let name = get_branch_name(&git_ref);
    let branch_commit = git_ref.peel_to_commit()?;
    let message: Vec<String> = String::from_utf8_lossy(branch_commit.message_bytes())
        .split_terminator("\n")
        .map(String::from)
        .collect();
    Ok(BranchInfo {
        name,
        commit_hash: branch_commit.id().to_string(),
        time: branch_commit.time().seconds(),
        message,
    })
}

pub fn git_list_branches(root: &Path) -> Result<BranchResponse, git2::Error> {
    let repo = Repository::open(root)?;

    let default_branch = repo
        .find_branch("origin/HEAD", BranchType::Remote)
        .ok()
        .map(|b| -> Result<String, git2::Error> {
            let git_ref = b.get().resolve()?;
            Ok(get_branch_name(&git_ref))
        })
        .transpose()?;

    let branches = repo
        .branches(Some(BranchType::Remote))?
        .filter(|branch_tuple| -> bool {
            if let Ok((b, _)) = branch_tuple {
                return b.name() != Ok(Some("origin/HEAD"));
            }
            true
        })
        .map(|branch_tuple| get_branch_info(branch_tuple?.0))
        .collect::<Result<Vec<BranchInfo>, git2::Error>>()?;

    Ok(BranchResponse {
        default_branch,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use test_utils::{git_get_latest_commit, git_remote_branches, initialise_git_repo};

    use super::*;

    #[test]
    fn can_perform_git_fetch() {
        let test_git = initialise_git_repo(None);

        let remote_ref = git_get_latest_commit(&test_git.remote, "HEAD");
        let initial_ref = git_get_latest_commit(&test_git.local, "refs/remotes/origin/HEAD");
        assert_ne!(
            initial_ref.message().unwrap(),
            remote_ref.message().unwrap()
        );

        let initial_branches = git_remote_branches(&test_git.local);
        assert_eq!(initial_branches.count(), 2); // HEAD and main

        git_fetch(&test_git.dir.path().join("local")).unwrap();

        let post_fetch_ref = git_get_latest_commit(&test_git.local, "refs/remotes/origin/HEAD");
        assert_eq!(
            post_fetch_ref.message().unwrap(),
            remote_ref.message().unwrap()
        );

        let post_fetch_branches = git_remote_branches(&test_git.local);
        assert_eq!(post_fetch_branches.count(), 3); // HEAD, main and other
    }

    #[test]
    fn can_list_git_branches() {
        let test_git = initialise_git_repo(None);
        let local_path = &test_git.dir.path().join("local");
        git_fetch(local_path).unwrap();

        let branch_response = git_list_branches(local_path).unwrap();
        let default_branch = branch_response.default_branch.unwrap();
        let branches_list = branch_response.branches;

        assert_eq!(default_branch, String::from("master"));

        assert_eq!(branches_list.len(), 2);
        assert_eq!(branches_list[0].name, String::from("master"));
        assert_eq!(
            branches_list[0].message,
            vec![String::from("Second commit")]
        );
        assert_eq!(branches_list[1].name, String::from("other"));
        assert_eq!(branches_list[1].message, vec![String::from("Third commit")]);
    }
}
