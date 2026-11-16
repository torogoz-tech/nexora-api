# /// script
# dependencies = ["jsonschema>=4,<5"]
# ///
"""Public schema contract; Rust validates capture references and UTF8 quotas."""
import copy
import json
from pathlib import Path
from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parent.parent
schema = json.loads((root / "schemas/lexer-v14.schema.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)
base = {"schema_version": 14, "identifier_kind": "word", "keywords": [], "tokens": [{"kind": "raw", "pattern": "(?P<arity>!+)~", "region": "raw"}], "regions": {"raw": {"close": "~", "counted_children": [{"capture": "arity", "unit": "!", "open": "{", "region": "hole"}]}, "hole": {"close": "}"}}}
assert validator.is_valid(base)
count = 1
for field in ["capture", "unit", "open", "region"]:
    for value, valid in [("", False), ("x" * 128, True), ("x" * 129, False), (None, False)]:
        model = copy.deepcopy(base)
        model["regions"]["raw"]["counted_children"][0][field] = value
        assert validator.is_valid(model) == valid, (field, value)
        count += 1
    model = copy.deepcopy(base)
    del model["regions"]["raw"]["counted_children"][0][field]
    assert not validator.is_valid(model)
    count += 1
for factor, valid in [(None, True), (1, False), (2, True), (16, True), (17, False)]:
    model = copy.deepcopy(base)
    model["regions"]["raw"]["counted_children"][0]["max_run_factor"] = factor
    assert validator.is_valid(model) == valid
    count += 1
for size in [0, 16, 17]:
    model = copy.deepcopy(base)
    model["regions"]["raw"]["counted_children"] *= size
    assert validator.is_valid(model) == (size <= 16)
    count += 1
older = copy.deepcopy(base)
older["schema_version"] = 13
old_validator = Draft202012Validator(json.loads((root / "schemas/lexer-v13.schema.json").read_text()))
assert not old_validator.is_valid(older)
del older["regions"]["raw"]["counted_children"]
assert old_validator.is_valid(older)
print(f"Counted transition schemas: {count + 2} field/version/boundary cases passed")
