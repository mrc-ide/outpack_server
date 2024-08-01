use std::path::Path;

use git2::{Branch, BranchType, Repository};
use serde::{Deserialize, Serialize};

use crate::config::read_config;

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
    name: Option<String>,
    commit_hash: String,
    time: i64,
    message: Option<String>,
}

fn get_branch_struct(
    branch_struct: Result<(Branch, BranchType), git2::Error>,
) -> Result<Branch, git2::Error> {
    Ok(branch_struct?.0)
}

fn get_branch_info(branch: Branch) -> Result<BranchInfo, git2::Error> {
    let name = branch.name()?.map(String::from);

    let branch_commit = branch.into_reference().peel_to_commit()?;
    let message = branch_commit.message().map(String::from);

    Ok(BranchInfo {
        name,
        commit_hash: branch_commit.id().to_string(),
        time: branch_commit.time().seconds(),
        message,
    })
}

fn io_err_to_git_err(err: std::io::Error) -> git2::Error {
    git2::Error::from_str(&err.to_string())
}

pub fn git_list_branches(root: &Path) -> Result<BranchResponse, git2::Error> {
    let repo = Repository::open(root)?;

    let default_branch_name = read_config(root)
        .map_err(io_err_to_git_err)?
        .git
        .default_branch;

    let default_branch_struct = match default_branch_name {
        Some(b) => repo.find_branch(&b, BranchType::Local),
        None => repo
            .find_branch("main", BranchType::Local)
            .or_else(|_| repo.find_branch("master", BranchType::Local)),
    }?;

    let default_branch = get_branch_info(default_branch_struct)?;

    let branches = repo
        .branches(Some(BranchType::Local))?
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

    use crate::config::{write_config, Config};

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
        let remote_path = &test_git.dir.path().join("remote");
        let outpack_path = &remote_path.join(".outpack");
        std::fs::create_dir(outpack_path).unwrap();

        let cfg = Config::new(None, true, true).unwrap();
        write_config(&cfg, &remote_path).unwrap();

        let branch_response = git_list_branches(&remote_path).unwrap();
        let now_in_seconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let default_branch = branch_response.default_branch;
        let branches_list = branch_response.branches;

        assert_eq!(default_branch.name, Some(String::from("master")));
        assert_eq!(default_branch.message, Some(String::from("Second commit")));
        assert_eq!(default_branch.time, now_in_seconds as i64);

        assert_eq!(branches_list.len(), 2);
        assert_eq!(branches_list[0].name, Some(String::from("master")));
        assert_eq!(
            branches_list[0].message,
            Some(String::from("Second commit"))
        );
        assert_eq!(branches_list[0].time, now_in_seconds as i64);
        assert_eq!(branches_list[1].name, Some(String::from("other")));
        assert_eq!(branches_list[1].message, Some(String::from("Third commit")));
        assert_eq!(branches_list[1].time, now_in_seconds as i64);
    }

    #[test]
    fn changes_default_branch_with_config() {
        let test_git = initialise_git_repo(None);
        let remote_path = &test_git.dir.path().join("remote");
        let outpack_path = &remote_path.join(".outpack");
        std::fs::create_dir(outpack_path).unwrap();

        let mut cfg = Config::new(None, true, true).unwrap();
        cfg.git.default_branch = Some(String::from("other"));
        write_config(&cfg, &remote_path).unwrap();

        let branch_response = git_list_branches(&remote_path).unwrap();
        let now_in_seconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let default_branch = branch_response.default_branch;
        let branches_list = branch_response.branches;

        assert_eq!(default_branch.name, Some(String::from("other")));
        assert_eq!(default_branch.message, Some(String::from("Third commit")));
        assert_eq!(default_branch.time, now_in_seconds as i64);

        assert_eq!(branches_list.len(), 2);
        assert_eq!(branches_list[0].name, Some(String::from("master")));
        assert_eq!(
            branches_list[0].message,
            Some(String::from("Second commit"))
        );
        assert_eq!(branches_list[0].time, now_in_seconds as i64);
        assert_eq!(branches_list[1].name, Some(String::from("other")));
        assert_eq!(branches_list[1].message, Some(String::from("Third commit")));
        assert_eq!(branches_list[1].time, now_in_seconds as i64);
    }
}
