"""HOLT desktop client."""
import os
import sys

# pythonw has no console streams. Kokoro/loguru registers sys.stderr during
# import, so provide valid sinks before importing any speech dependencies.
# Application errors are recorded separately through client.diagnostics.
if sys.stdout is None:
    sys.stdout = open(os.devnull, "w", encoding="utf-8")
if sys.stderr is None:
    sys.stderr = open(os.devnull, "w", encoding="utf-8")
