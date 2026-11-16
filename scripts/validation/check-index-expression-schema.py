# /// script
# dependencies = ["jsonschema>=4,<5", "PyYAML>=6,<7"]
# ///
"""Validate public index-expression schema syntax; Rust checks model references."""
from pathlib import Path
import copy
import json
import yaml
from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parent.parent
schemas = {version:json.loads((root/f'schemas/grammar-v{version}.schema.json').read_text()) for version in [7,8]}
for schema in schemas.values():
    Draft202012Validator.check_schema(schema)
Draft202012Validator.check_schema(json.loads((root/'schemas/corpus/case.schema.json').read_text()))
base = yaml.safe_load((root/'templates/semantic-molds/brace_mold/grammar.yaml').read_text())
base['schema_version'] = 8
base['groups'].append({'open':'[','close':']','kind':'delimiter'})
operator = {'open':'[','close':']','kind':'delimiter','separator':{'text':',','kind':'delimiter'},'precedence':80,'expression_kind':'index','max_arguments':1}
base['contexts']['expression']['expression']['application'].append(operator)
cases = [('index',1,True),('index',0,False),('index',1024,True),('index',1025,False),('index',-1,False),('application',0,True),('literal',1,False),(None,1,True),('index',None,True)]
for kind,maximum,valid in cases:
    model = copy.deepcopy(base)
    operation=model['contexts']['expression']['expression']['application'][-1]
    operation.update(expression_kind=kind,max_arguments=maximum)
    assert Draft202012Validator(schemas[8]).is_valid(model)==valid,(kind,maximum)
older = copy.deepcopy(base)
older['schema_version']=7
assert not Draft202012Validator(schemas[7]).is_valid(older)
operation=older['contexts']['expression']['expression']['application'][-1]
del operation['expression_kind'];del operation['max_arguments']
assert Draft202012Validator(schemas[7]).is_valid(older)
print(f'Index expression schemas: {len(cases)+2} version/kind/arity cases passed')
