use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn prints_usage_if_args_invalid() {
    let mut cmd = Command::cargo_bin("outpack").unwrap();
    cmd.assert().stderr(predicate::str::contains("Usage:"));
}
