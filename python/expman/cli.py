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
    # Detect if we are running from a source checkout or installed package
    expman_dir = Path(__file__).parent

    # Potential locations for the binary
    # 1. expman/bin/exp (maturin default bundle location)
    # 2. expman/exp (fallback if not in bin/)
    # 3. .env/bin/exp (if installed via pip)

    potential_binaries = [
        expman_dir / "bin" / ("exp.exe" if sys.platform == "win32" else "exp"),
        expman_dir / ("exp.exe" if sys.platform == "win32" else "exp"),
    ]

    for binary in potential_binaries:
        if binary.exists():
            if sys.platform != "win32":
                os.chmod(binary, 0o755)
            sys.exit(subprocess.call([str(binary)] + sys.argv[1:]))

    # Fall back to PATH (e.g. when running in development with `cargo install`)
    try:
        sys.exit(subprocess.call(["exp"] + sys.argv[1:]))
    except FileNotFoundError:
        print(
            f"Error: 'exp' binary not found at {potential_binaries} or in PATH.\n"
            "Please ensure expman-rs is correctly installed.\n"
            "Try: pip install --force-reinstall expman-rs",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
