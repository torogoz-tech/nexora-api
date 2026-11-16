# /// script
# dependencies = ["jsonschema>=4,<5"]
# ///
"""Check the independent EOF contract of lexer9..14 and local token rules."""
import copy
import json
from pathlib import Path
from jsonschema import Draft202012Validator

root=Path(__file__).resolve().parent.parent
model={'schema_version':9,'identifier_kind':'word','keywords':[], 'tokens':[{'kind':'word','pattern':'[a-z]+'}], 'regions':{'body':{'close':'','close_before':[{'pattern':';'}]}}}
cases=[
    ({'close':'','close_before':[{'pattern':';'}]},True),
    ({'close':'','close_before':[{'pattern':';'}],'eof_closes':False},True),
    ({'close':'','close_before':[{'pattern':';'}],'eof_closes':True},True),
    ({'close':'','eof_closes':True},True),
    ({'close':'','close_before':[],'eof_closes':True},True),
    ({'close':''},False),
    ({'close':'','close_before':[]},False),
    ({'close':'','eof_closes':False},False),
    ({'close':'','close_before':[{'pattern':';'}],'alternate_closes':['END']},False),
    ({'close':'','close_before':[{'pattern':';'}],'close_capture':'name'},False),
    ({'close':'','close_before':[{'pattern':';'}],'close_tail':'END'},False),
]
for version in (9, 10, 11, 12, 13, 14):
    schema=json.loads((root/f'schemas/lexer-v{version}.schema.json').read_text())
    Draft202012Validator.check_schema(schema)
    validator=Draft202012Validator(schema)
    model['schema_version']=version
    for region,valid in cases:
        candidate=copy.deepcopy(model);candidate['regions']['body']=region
        assert validator.is_valid(candidate)==valid, (version, region)
    for value in (True, False, 'false', 0, None):
        candidate=copy.deepcopy(model);candidate['tokens'][0]['root']=value
        assert validator.is_valid(candidate)==(version>=10 and isinstance(value, bool)), (version, value)
    print(f'lexer{version} schema: {len(cases)} EOF/boundary and 5 root scope cases passed')
