# outpack

[![Project Status: Concept – Minimal or no implementation has been done yet, or the repository is only intended to be a limited example, demo, or proof-of-concept.](https://www.repostatus.org/badges/latest/concept.svg)](https://www.repostatus.org/#concept)

Rust implementation of `outpack`.

## Usage
This crate provides an `outpack` command which can be used to create and operate on Outpack repositories.

The command can be built with `cargo build`, after which it can be found in `target/debug/outpack`.
Alternatively it can be started directly by using `cargo run`.

### Initializing a new repository

```
outpack init --use-file-store <path>
```


### Query CLI usage

```
outpack query --root <path> <query>
```

### API Server

The `outpack` command includes an API server which can be used to expose the
repository over an HTTP interface.

```
outpack start-server --root <path>
```

## Usage of docker image

```
docker run --name outpack_server -v /full/path/to/root:/outpack -p 8000:8000 -d ghcr.io/mrc-ide/outpack_server:main
```

## Schema

The outpack schema is imported into this package by running `./scripts/import_schema`,
and needs to be kept manually up to date by re-running that script as needed.

## Tests

Run all tests with `cargo test`.

## API
### GET /

```json
{
   "status": "succcess",
   "data": {
        "schema_version": "0.0.1"
   },
   "errors": null
}
```

### GET /checksum

Returns hash of all current packet ids, ordered alphanumerically and concatenated. This will use the hashing algorithm specified 
in the `outpack` config, unless a query parameter specifying an alternative is passed: 
e.g. `/checksum?alg=md5`. 

```json
{
   "status": "succcess",
   "data": "md5:117723186364b4b409081b1bd347d406",
   "errors": null
}
```

### GET /metadata/list

```json
{
    "status": "success",
    "errors": null,
    "data": [
        {
            "packet": "20220812-155808-c873e405",
            "time": "2022-08-12 15:58:08",
            "hash": "sha256:df6edb3d6cd50f5aec9308a357111592cde480f45a5f46341877af21ae30d93e"
        },
        {
            "packet": "20220812-155808-d5747caf",
            "time": "2022-08-12 15:58:08",
            "hash": "sha256:edc70ef51e69f2cde8238142af29a9419bb27c94b320b87e88f617dfc977a46b"
        },
        {
            "packet": "20220812-155808-dbd3ce81",
            "time": "2022-08-12 15:58:08",
            "hash": "sha256:a7da8c3464a2da4722b9d15daa98eb13f4f8c1949c6d00100428b2e9d0668f29"
        },
        {
            "packet": "20220812-155808-e21bc5fc",
            "time": "2022-08-12 15:58:08",
            "hash": "sha256:df1b91aaf3393483515ac61596aa35117891eacc533a55ec2f4759d0036514f9"
        }
    ]
}
```

### GET /packit/metadata

Returns a list of (truncated) packet metadata. 
Accepts an optional query parameter `known_since` specifying a Unix epoch time 
from which to return results. This will filter packets by the `time` property of the 
location metadata, i.e. the point at which they were inserted into the index.
e.g. `/packit/metadata?known_since=1683117048`. 

```json
{
    "status": "success",
    "errors": null,
    "data": [
        {
            "id": "20220812-155808-c873e405",
            "name": "depends",
            "parameters": null,
            "time": {
              "end": 1503074545.8687,
              "start": 1503074545.8687
            },
            "custom": { "orderly": { "description": { "display": "Report with dependencies" }}},
        },
        {
            "id": "20220812-155808-d5747caf",
            "name": "params",
            "parameters": { "alpha": 1 },
            "time": {
              "start": 1722267993.0676,
              "end": 1722267993.0971
            },
            "custom": { "orderly": { "description": { "display": "Report with parameters" }}},
        }
    ]
}
```


### GET /metadata/\<id\>/json

```json
{
  "status": "success",
  "errors": null,
  "data": {
    "custom": null,
    "depends": [],
    "files": [
      {
        "hash": "sha256:e9aa9f2212aba6fba4464212800a2927afa02eda688cf13131652da307e3d7c1",
        "path": "orderly.yml",
        "size": 955
      },
      {
        "hash": "sha256:11a2cd93493defa673b198d5be7a180cef7b133baaacc046923e1e2da77c6f75",
        "path": "modified_update.R",
        "size": 1133
      },
      {
        "hash": "sha256:c4d4c95af9da912f2f20c65a0502c7da19a5712767a39e07a2dd1ea7fcb615b0",
        "path": "R/util.R",
        "size": 2757
      }
    ],
    "id": "20170818-164043-7cdcde4b",
    "name": "modup-201707",
    "parameters": null,
    "schema_version": "0.0.1",
    "script": [
      "modified_update.R"
    ],
    "session": {
      "packages": [
        {
          "attached": true,
          "package": "RcppRoll",
          "version": "0.2.2"
        },
        {
          "attached": false,
          "package": "Rcpp",
          "version": "0.12.12"
        },
        {
          "attached": false,
          "package": "ids",
          "version": "1.0.1"
        }
      ],
      "platform": {
        "os": "Debian GNU/Linux 9 (stretch)",
        "system": "x86_64, linux-gnu",
        "version": "R version 3.4.0 (2017-04-21)"
      }
    },
    "time": {
      "end": 1503074545.8687,
      "start": 1503074545.8687
    }
  }
}
```

### GET /metadata/\<id\>/text

Returns the same as `GET /metadata/<id>/json` but just the data as plain text.

### GET /file/\<hash\>

Downloads the file with the provided hash. 404 if it doesn't exist.

### POST /packets/missing

#### Body

```json
{
    "ids": ["20220812-155808-c873e405","20220812-155808-d5747caf"],
    "unpacked": false
}
```

Given a list of ids, returns those that are missing in the current root. If `unpacked` is true
returns missing unpacked packets, otherwise just looks at missing metadata. 

#### Response
```json
{
  "status": "success",
  "errors": null,
  "data": ["20220812-155808-c873e405", "20220812-155808-d5747caf"]
}
```

### POST /files/missing

#### Body
```json
{
  "hashes": [
    "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248",
    "sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d247"
  ]
}
```

Given a list of file hashes, returns those that are missing in the current root.

#### Response
```json
{
  "status": "success",
  "errors": null,
  "data": ["sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d24"]
}
```

### POST /file/<hash>

Upload a file with the given hash. Returns a 400 if the hash does not match the file contents.
This method is idempotent; if the file already exists it will not do anything.

#### Body

The file contents should be written directly to the request body.

#### Response

```json
{
  "status": "success",
  "errors": null,
  "data": null
}
```

### POST /packet/<hash>

Upload packet metadata with the given hash. Returns a 400 if the hash does not match the contents.
This method is idempotent; if the file already exists it will not do anything.

#### Body

The metadata should be written directly to the request body.

#### Response

```json
{
  "status": "success",
  "errors": null,
  "data": null
}
```

### POST /git/fetch

Does a git fetch on the repository (relevant for when runners clone down git repositories). Expects an empty json body.

### GET /git/branches

Returns an array of branches with their `name`, `commit_hash` (where branch pointer is), `time` (of last commit) and `message` (of last commit in a string array split with respect to newline characters)

#### Response

```json
{
    "status": "success",
    "data": {
        "default_branch": "main",
        "branches": [
            {
              "name": "main",
              "commit_hash": "ede307e23b2137ba2c7c3270e52f354f224942af",
              "time": 1722436575,
              "message": ["First commit"]
            },
            {
              "name": "other",
              "commit_hash": "e9078cf779584168c3781379380a3b1352545cda",
              "time": 1722436640,
              "message": ["Second commit"]
            }
        ]
    },
    "errors": null
}
```

## Python bindings

This crate provides Python bindings for its query parser. See
[README.python.md](README.python.md) for details.

## Releasing

- Increment the version field in `Cargo.toml`.
- Run `cargo fetch` to update the version field of `Cargo.lock`.
- Create a new pull request with these changes.
- Get the PR approved and merged to main.
- Create a [GitHub release](https://github.com/mrc-ide/outpack_server/releases/new):
  - Set the tag name as `vX.Y.Z`, matching the version used in `Cargo.toml`.
  - Write some release notes (possibly using the `Generate release notes` button).
  - Publish the release!
- Sit back and relax while the release gets built and published.
- Check that the new version is available on [PyPI](https://pypi.org/project/outpack-query-parser/#history).

## License

MIT © Imperial College of Science, Technology and Medicine
