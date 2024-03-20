use std::path::Path;

use git2::Repository;

pub fn git_fetch(root: &Path) -> Result<(), git2::Error> {
    let repo = Repository::open(root)?;
    let mut remote = repo
        .find_remote("origin")
        .expect("Failed to find remote 'origin'");
    let ref_specs_iter = remote
        .fetch_refspecs()
        .expect("Failed to get remotes ref specs");
    let ref_specs: Vec<&str> = ref_specs_iter.iter().map(|spec| spec.unwrap()).collect();
    remote
        .fetch(&ref_specs, None, None)
        .expect("Failed to fetch");
    Ok(())
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

        git_fetch(&test_git.dir.join("local")).unwrap();

        let post_fetch_ref = git_get_latest_commit(&test_git.local, "refs/remotes/origin/HEAD");
        assert_eq!(
            post_fetch_ref.message().unwrap(),
            remote_ref.message().unwrap()
        );

        let post_fetch_branches = git_remote_branches(&test_git.local);
        assert_eq!(post_fetch_branches.count(), 3); // HEAD, main and other
    }
}
