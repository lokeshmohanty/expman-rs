import os
import subprocess
import sys


def main():
    """
    Minimal shim to launch the bundled Rust exp binary.
    """
    # The binary is bundled in the 'bin' subdirectory of the package
    bin_name = "exp.exe" if sys.platform == "win32" else "exp"
    bin_path = os.path.join(os.path.dirname(__file__), "bin", bin_name)

    if not os.path.exists(bin_path):
        print(f"Error: Bundled binary not found at {bin_path}", file=sys.stderr)
        return 1

    # Ensure it's executable (maturin/pip usually handles this, but good to be safe)
    if not os.access(bin_path, os.X_OK):
        try:
            os.chmod(bin_path, 0o755)
        except Exception:
            pass

    # Forward all arguments and return the exit code
    return subprocess.call([bin_path] + sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
