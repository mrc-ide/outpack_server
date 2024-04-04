# outpack_query_parser

Python bindings for the outpack query parser. The bindings can be installed
directly from PyPI using `pip install outpack_query_parser`.

This package provides a low-level building block for the Python version of
outpack/orderly. Most users shouldn't need to use this package and should
instead install [outpack-py](https://github.com/mrc-ide/outpack-py), which
provides the high-level functionality.

## Development

```
hatch run python  # Start a Python interpreter with the bindings installed
hatch run test    # Run the bindings test-suite
hatch run develop # Rebuild the bindings. Necessary whenever changes to Rust code is made.
```
