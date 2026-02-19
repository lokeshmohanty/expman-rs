import subprocess
import sys


def main():
    """
    Entry point for the 'expman' command when installed via pip.
    This wraps the Rust binary which should be bundled or available in the path.
    """
    # In a real maturin-built package, we might bundle the binary or
    # expect it to be in the same directory as the package.
    # For now, we'll try to find the binary that maturin might have installed
    # or just call 'expman' and hope it's on the PATH.

    # Ideally, expman-rs should bundle the binary.
    # Since this is a hybrid package, we can also implement some logic here if needed.
    # But often maturin users prefer to use the Rust CLI directly.

    try:
        # Check if expman is available in the path
        # If not, we might need a more sophisticated way to find the bundled binary.
        result = subprocess.run(["expman"] + sys.argv[1:])
        sys.exit(result.returncode)
    except FileNotFoundError:
        print("Error: 'expman' binary not found. Please ensure 'expman' is installed.")
        sys.exit(1)


if __name__ == "__main__":
    main()
