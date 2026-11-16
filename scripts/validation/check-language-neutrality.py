#!/usr/bin/env python3
"""Regression guard: registered language IDs cannot be hard-coded in runtime Rust."""
from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parent.parent
# IDs are read from the simple scalar field, without a YAML dependency.
ids = []
for manifest in (ROOT / "languages").glob("*/language.yaml"):
    match = re.search(r"^id:\s*([\w.-]+)\s*$", manifest.read_text(), re.MULTILINE)
    if match:
        ids.append(match.group(1))
violations = []
for source in (ROOT / "crates").glob("*/src/**/*.rs"):
    text = source.read_text()
    for identifier in ids:
        if json.dumps(identifier) in text:
            violations.append(f"{source.relative_to(ROOT)} embeds language ID {identifier!r}")
    for retired in ["ModuleImportParser", "ModuleImportRule", "from_keyword", "alias_keyword"]:
        if retired in text:
            violations.append(f"{source.relative_to(ROOT)} embeds retired dialect helper {retired}")
if violations:
    raise SystemExit("\n".join(violations))
print(f"Language-neutral runtime guard passed for {len(ids)} discovered packs.")
