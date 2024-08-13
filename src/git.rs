use std::path::Path;

use git2::{Branch, BranchType, Repository};
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
    default_branch: BranchInfo,
    branches: Vec<BranchInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct BranchInfo {
    name: String,
    commit_hash: String,
    time: i64,
    message: Vec<String>,
}

fn get_branch_struct(
    branch_struct: Result<(Branch, BranchType), git2::Error>,
) -> Result<Branch, git2::Error> {
    Ok(branch_struct?.0)
}

fn get_branch_info(branch: Branch) -> Result<BranchInfo, git2::Error> {
    let lossy_name = String::from_utf8_lossy(branch.name_bytes()?);
    let name = lossy_name
        .strip_prefix("origin/")
        .unwrap_or(&lossy_name)
        .to_owned();

    let branch_commit = branch.into_reference().peel_to_commit()?;
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
    let default_branch_buf = repo.find_remote("origin")?.default_branch()?;

    let default_branch_name = match default_branch_buf.as_str() {
        Some(b) => Ok(b),
        None => Err(git2::Error::from_str("Could not find default branch")),
    }?;

    let default_branch_struct = repo.find_branch(default_branch_name, BranchType::Remote)?;
    let default_branch = get_branch_info(default_branch_struct)?;

    let branches = repo
        .branches(Some(BranchType::Remote))?
        // first branch seems to be HEAD, we don't want to display that to the
        // users so skip it
        .skip(1)
        .map(get_branch_struct)
        .collect::<Result<Vec<Branch>, git2::Error>>()?
        .into_iter()
        .map(get_branch_info)
        .collect::<Result<Vec<BranchInfo>, git2::Error>>()?;

    Ok(BranchResponse {
        default_branch,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

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
        let now_in_seconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let default_branch = branch_response.default_branch;
        let branches_list = branch_response.branches;

        assert_eq!(default_branch.name, String::from("master"));
        assert_eq!(default_branch.message, vec![String::from("First commit")]);
        assert_eq!(default_branch.time, now_in_seconds as i64);

        assert_eq!(branches_list.len(), 2);
        assert_eq!(branches_list[0].name, String::from("master"));
        assert_eq!(
            branches_list[0].message,
            vec![String::from("Second commit")]
        );
        assert_eq!(branches_list[0].time, now_in_seconds as i64);
        assert_eq!(branches_list[1].name, String::from("other"));
        assert_eq!(branches_list[1].message, vec![String::from("Third commit")]);
        assert_eq!(branches_list[1].time, now_in_seconds as i64);
    }

    #[test]
    fn changes_default_branch_with_config() {
        let test_git = initialise_git_repo(None);
        let local_path = &test_git.dir.path().join("local");
        git_fetch(local_path).unwrap();

        let branch_response = git_list_branches(local_path).unwrap();
        let now_in_seconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let default_branch = branch_response.default_branch;
        let branches_list = branch_response.branches;

        assert_eq!(default_branch.name, String::from("other"));
        assert_eq!(default_branch.message, vec![String::from("Third commit")]);
        assert_eq!(default_branch.time, now_in_seconds as i64);

        assert_eq!(branches_list.len(), 2);
        assert_eq!(branches_list[0].name, String::from("master"));
        assert_eq!(
            branches_list[0].message,
            vec![String::from("Second commit")]
        );
        assert_eq!(branches_list[0].time, now_in_seconds as i64);
        assert_eq!(branches_list[1].name, String::from("other"));
        assert_eq!(branches_list[1].message, vec![String::from("Third commit")]);
        assert_eq!(branches_list[1].time, now_in_seconds as i64);
    }
}
