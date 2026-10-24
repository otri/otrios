#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Compatibility entry point; use publish-app.py for UI and behavior updates."""
from pathlib import Path
from runpy import run_path
globals().update(run_path(str(Path(__file__).with_name('publish-app.py')),run_name=__name__))
