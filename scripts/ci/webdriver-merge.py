"""Print a webdriver.json that adds Chrome arguments to an existing one.

usage: python3 -I webdriver-merge.py <base webdriver.json or ""> <chrome arg>...

wasm-coverage.sh gives each suite's Chrome a --remote-debugging-port. The
repository's own webdriver.json (the 1140x900 window the layout tests rely
on) must stay in force, so its capabilities are kept and the new arguments
are appended to goog:chromeOptions.args. An argument already present is not
repeated. A missing or empty base path means no base.
"""
import json
import os
import sys

base_path, extra = sys.argv[1], sys.argv[2:]
config = {}
if base_path and os.path.exists(base_path):
    with open(base_path, encoding="utf-8") as f:
        config = json.load(f)
if not isinstance(config, dict):
    sys.exit(f"webdriver-merge: {base_path} is not a JSON object")
options = config.setdefault("goog:chromeOptions", {})
args = options.setdefault("args", [])
args.extend(a for a in extra if a not in args)
json.dump(config, sys.stdout, indent=2)
sys.stdout.write("\n")
