#!/usr/bin/env python3
"""Find Python 3.9+ installation on Windows."""
import sys
import subprocess
import os
import glob

# First check our own version
print(f"Current python: {sys.executable}")
print(f"Version: {sys.version}")

# On Windows, check common locations
common_paths = [
    r"C:\Python311\python.exe",
    r"C:\Python310\python.exe",
    r"C:\Python39\python.exe",
    r"C:\Users\11428\AppData\Local\Programs\Python\Python311\python.exe",
    r"C:\Users\11428\AppData\Local\Programs\Python\Python310\python.exe",
    r"C:\Users\11428\AppData\Local\Programs\Python\Python39\python.exe",
    r"C:\Program Files\Python311\python.exe",
    r"C:\Program Files\Python310\python.exe",
    r"C:\Program Files\Python39\python.exe",
]

for path in common_paths:
    if os.path.exists(path):
        try:
            result = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=5)
            print(f"Found: {path} -> {result.stdout.strip()}")
        except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
            pass

# Check PATH
print("\nPython-related PATH entries:")
path_env = os.environ.get("PATH", "")
for p in path_env.split(";"):
    if "python" in p.lower():
        print(f"  {p}")
