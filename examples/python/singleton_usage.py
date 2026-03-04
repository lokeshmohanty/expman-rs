"""
Singleton usage example for expman-rs.
This demonstrates tracking without a context manager (using expman.init()).
"""

import random
import struct
import time
import zlib

import expman as exp


def train():
    # 1. Initialize global experiment (replaces with Experiment(...) as exp)
    exp.init("singleton_example")

    print("Initiated global experiment.")

    # 2. Log parameters globally
    exp.log_params({"lr": 0.01, "batch_size": 32, "singleton": True})

    # 3. Log metrics anywhere without passing an 'exp' object
    for step in range(10):
        val = random.random()
        exp.log_vector({"accuracy": val}, step=step)
        print(f"Step {step}: acc={val:.4f}")
        time.sleep(0.2)

    # 4. Save text artifact globally
    with open("notes.txt", "w") as f:
        f.write("Using the singleton API!")
    exp.save_artifact("notes.txt")

    # 5. Generate and save a dummy image artifact (stdlib-only PNG)
    width, height = 64, 64
    raw = b""
    for y in range(height):
        raw += b"\x00"  # filter byte
        for x in range(width):
            raw += struct.pack("BBB", int(255 * x / width), int(255 * y / height), 128)

    def _chunk(tag: bytes, data: bytes) -> bytes:
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += _chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += _chunk(b"IDAT", zlib.compress(raw))
    png += _chunk(b"IEND", b"")
    with open("gradient.png", "wb") as f:
        f.write(png)
    exp.save_artifact("gradient.png")
    print("Saved image artifact.")

    # 5. Optional manual close (atexit handles it otherwise)
    exp.close()
    print("Experiment finished and closed.")


if __name__ == "__main__":
    train()
