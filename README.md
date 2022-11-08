# outpack_server
[![Project Status: Concept – Minimal or no implementation has been done yet, or the repository is only intended to be a limited example, demo, or proof-of-concept.](https://www.repostatus.org/badges/latest/concept.svg)](https://www.repostatus.org/#concept)

Rust implementation of the `outpack` HTTP API.

## Usage
Start an API for `outpack` root with `cargo run -- --root <path>`.

E.g.

```cargo run -- --root tests/example```

## Tests
Run all tests with `cargo test`.

## GET /

```
{
   "schema_version": "0.1.4"
}
```

## GET /metadata/list

```
[
    {
        "packet": "20220812-155808-c873e405",
        "time": "2022-08-12 15:58:08",
        "hash": "sha256:df6edb3d6cd50f5aec9308a357111592cde480f45a5f46341877af21ae30d93e"
    },
    {
        "packet": "20220812-155808-d5747caf",
        "time": "2022-08-12 15:58:08",
        "hash": "sha256:edc70ef51e69f2cde8238142af29a9419bb27c94b320b87e88f617dfc977a46b"
    }
]
```
