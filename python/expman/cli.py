import os
import subprocess
import sys
from pathlib import Path


def main():
    """
    Entry point for the 'exp' command when installed via pip.
    Uses the Rust binary bundled by maturin under expman/bin/exp.
    Falls back to PATH if the bundled binary is not found.
    """
    # Try the bundled binary first (placed here by maturin)
    bin_dir = Path(__file__).parent / "bin"
    binary = bin_dir / ("exp.exe" if sys.platform == "win32" else "exp")

    if binary.exists():
        if sys.platform != "win32":
            os.chmod(binary, 0o755)
        sys.exit(subprocess.call([str(binary)] + sys.argv[1:]))

    # Fall back to PATH (e.g. when running in development with `cargo install`)
    try:
        sys.exit(subprocess.call(["exp"] + sys.argv[1:]))
    except FileNotFoundError:
        print(
            "Error: 'exp' binary not found. "
            "Install via 'cargo install expman-cli' or reinstall the Python package.",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
