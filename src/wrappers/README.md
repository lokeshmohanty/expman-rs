# ExpMan Language Wrappers

The `wrappers` module contains the source code for language-specific bindings to the core Rust `expman` library.

## Python Wrapper

The Python wrapper is located in `src/wrappers/python/`. It uses `PyO3` to create a compiled extension module that provides an idiomatic Python interface to the high-performance Rust engine.

For the Python package source code (including the CLI shim and tests), see the top-level [`wrappers/python/`](../../wrappers/python/) directory.

## Future Wrappers

This module is designed to be extensible, allowing for the addition of other language bindings (e.g., C++, Julia) in the future while sharing the same core logic.
