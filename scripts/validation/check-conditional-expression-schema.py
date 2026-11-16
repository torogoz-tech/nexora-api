# /// script
# dependencies = ["jsonschema>=4,<5", "PyYAML>=6,<7"]
# ///
"""Validate the public conditional schema; Rust checks lexical references."""
from pathlib import Path
import copy
import json
import yaml
from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parent.parent
schemas = {v: json.loads((root / f"schemas/grammar-v{v}.schema.json").read_text()) for v in [8, 9]}
for schema in schemas.values():
    Draft202012Validator.check_schema(schema)
base = yaml.safe_load((root / "templates/web-understanding/typescript/grammar.yaml").read_text())
validator = Draft202012Validator(schemas[9])
assert validator.is_valid(base)
count = 1
for field, values in {
    "precedence": [(0, False), (1, True), (1024, True), (1025, False)],
    "consequent_precedence": [(-1, False), (0, True), (1024, True), (1025, False)],
    "alternative_precedence": [(-1, False), (0, True), (1024, True), (1025, False)],
}.items():
    for value, valid in values:
        model = copy.deepcopy(base)
        model["contexts"]["expression"]["expression"]["conditional"][0][field] = value
        assert validator.is_valid(model) == valid, (field, value)
        count += 1
for field in ["token", "separator", "precedence", "consequent_precedence", "alternative_precedence"]:
    model = copy.deepcopy(base)
    del model["contexts"]["expression"]["expression"]["conditional"][0][field]
    assert not validator.is_valid(model), field
    count += 1
for size in [0, 16, 17]:
    model = copy.deepcopy(base)
    expression = model["contexts"]["expression"]["expression"]
    expression["conditional"] = expression["conditional"] * size
    assert validator.is_valid(model) == (size <= 16), size
    count += 1
older = copy.deepcopy(base)
older["schema_version"] = 8
assert not Draft202012Validator(schemas[8]).is_valid(older)
del older["contexts"]["expression"]["expression"]["conditional"]
assert Draft202012Validator(schemas[8]).is_valid(older)
print(f"Conditional expression schemas: {count + 2} version/field/boundary cases passed")
