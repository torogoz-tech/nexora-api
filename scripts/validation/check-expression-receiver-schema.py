# /// script
# dependencies = ["jsonschema>=4,<5", "PyYAML>=6,<7"]
# ///
"""Check receiver-filter schema syntax; Rust validates references and totals."""
from pathlib import Path
import copy
import json
import yaml
from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parent.parent
schemas = {v: json.loads((root / f"schemas/grammar-v{v}.schema.json").read_text()) for v in [9, 10]}
for schema in schemas.values():
    Draft202012Validator.check_schema(schema)
base = yaml.safe_load((root / "templates/semantic-molds/brace_mold/grammar.yaml").read_text())
base["schema_version"] = 10
validator = Draft202012Validator(schemas[10])
assert validator.is_valid(base)
cases = [
    ({"kinds": ["group"]}, True),
    ({"kinds": []}, False),
    ({"kinds": ["group", "group"]}, False),
    ({"kinds": ["unknown"]}, False),
    ({"operator": "("}, False),
    ({"kinds": ["group"], "operator": ""}, False),
    ({"kinds": ["group"], "operator": "x" * 129}, False),
    ({"kinds": ["group"], "descendant_kinds": []}, True),
    ({"kinds": ["group"], "descendant_operators": []}, True),
    ({"kinds": ["group"], "descendant_operators": [".", "."]}, False),
    ({"kinds": ["identifier"], "token": {"text": "value", "kind": "identifier"}}, True),
    ({"kinds": ["group"], "unexpected": True}, False),
    ({"kinds": ["group"], "operator": None, "token": None, "descendant_kinds": None, "descendant_operators": None}, True),
]
for value, expected in cases:
    model = copy.deepcopy(base)
    model["contexts"]["expression"]["expression"]["application"][0]["excluded_receivers"] = [value]
    assert validator.is_valid(model) == expected, value
for size in [0, 16, 17]:
    model = copy.deepcopy(base)
    model["contexts"]["expression"]["expression"]["application"][0]["excluded_receivers"] = [{"kinds": ["group"]}] * size
    assert validator.is_valid(model) == (size <= 16), size
older = copy.deepcopy(base)
older["schema_version"] = 9
assert Draft202012Validator(schemas[9]).is_valid(older)
older["contexts"]["expression"]["expression"]["application"][0]["excluded_receivers"] = [{"kinds": ["group"]}]
assert not Draft202012Validator(schemas[9]).is_valid(older)
print(f"Receiver-filter schemas: {len(cases) + 6} field/boundary/version cases passed")
