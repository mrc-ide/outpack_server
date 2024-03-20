# test-utils

This exists as a separate crate so that we can use it for test setup of both integration and unit tests in the main
outpack crate.

There are also test utils in `outpack_server/src/test_utils.rs` It would be nice to move all test utils here but some of
the utils reference internals of the outpack crate which we cannot include here.

Moving forward we should:

* Put utils for integration and unit tests in this crate
* Put utils only for unit tests in `outpack_server/src/test_utils.rs`
* Put utils only for integration testing either here or in `tests` dir itself

