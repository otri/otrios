#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run the builder and publishing tools from the configured source checkout."""
import argparse
import json
import pathlib
import runpy


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument('--config', type=pathlib.Path, required=True)
    args, _ = parser.parse_known_args()
    config = json.loads(args.config.read_text())
    entry = pathlib.Path(config['repo']) / 'scripts/watch-dashboard.py'
    if not entry.is_file():
        raise SystemExit('Configured checkout is missing scripts/watch-dashboard.py')
    runpy.run_path(str(entry), run_name='__main__')


if __name__ == '__main__':
    main()
