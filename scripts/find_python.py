#!/usr/bin/env python3
"""Find Python 3.9+ installation on Windows."""
import sys
import subprocess
import os
import glob

# First check our own version
print(f"Current python: {sys.executable}")
print(f"Version: {sys.version}")

# On Windows, check common locations（动态构建，不硬编码用户名）
_localappdata = os.environ.get("LOCALAPPDATA", "")
_programfiles = os.environ.get("ProgramFiles", r"C:\Program Files")
common_paths = []
# %LOCALAPPDATA%\Programs\Python\PythonXY（每用户安装，动态）
if _localappdata:
    for _xy in ("313", "312", "311", "310", "39"):
        common_paths.append(os.path.join(_localappdata, "Programs", "Python", f"Python{_xy}", "python.exe"))
# C:\PythonXY（系统级安装）
for _xy in ("313", "312", "311", "310", "39"):
    common_paths.append(rf"C:\Python{_xy}\python.exe")
# %ProgramFiles%\PythonXY
for _xy in ("313", "312", "311", "310", "39"):
    common_paths.append(os.path.join(_programfiles, f"Python{_xy}", "python.exe"))

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
