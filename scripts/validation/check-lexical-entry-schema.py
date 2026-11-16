# /// script
# dependencies = ["jsonschema>=4,<5"]
# ///
"""Check public syntax of lexical entries and route selectors; Rust checks references."""
import copy
import json
from pathlib import Path
from jsonschema import Draft202012Validator

root=Path(__file__).resolve().parent.parent
schemas={name:json.loads((root/'schemas'/name).read_text()) for name in ['lexer-v12.schema.json','lexer-v13.schema.json','lexer-v14.schema.json','language-v1.schema.json']}
for schema in schemas.values():Draft202012Validator.check_schema(schema)
model={'schema_version':13,'identifier_kind':'word','keywords':[],'tokens':[{'kind':'word','pattern':'[a-z]+'}],'entry_contexts':{'expr':['word']}}
cases=[({'expr':['word']},True),({},True),({'': ['word']},False),({'bad/name':['word']},False),({'x'*65:['word']},False),({'expr':[]},False),({'expr':['word','word']},False),({'expr':[1]},False),({f'entry{i}':['word'] for i in range(64)},True),({f'entry{i}':['word'] for i in range(65)},False)]
for version in [13,14]:
 validator=Draft202012Validator(schemas[f'lexer-v{version}.schema.json'])
 for entries,valid in cases:
  candidate=copy.deepcopy(model);candidate['schema_version']=version;candidate['entry_contexts']=entries
  assert validator.is_valid(candidate)==valid,entries
older=copy.deepcopy(model);older['schema_version']=12
assert not Draft202012Validator(schemas['lexer-v12.schema.json']).is_valid(older)
older.pop('entry_contexts');assert Draft202012Validator(schemas['lexer-v12.schema.json']).is_valid(older)
manifest={'schema_version':1,'id':'host','version':'0.1.0','extensions':['host'],'composition':{'schema_version':6,'roles':{'island':'grammar'},'routes':{'island':{'language':'inner','context':'expression','lexical_context':'expr','scope':{'visibility':'whole_scope','parent_lookup':False,'shadowing':'nearest'}}}}}
validator=Draft202012Validator(schemas['language-v1.schema.json'])
for selection,valid in [('expr',True),(None,True),('',False),('bad/name',False),('x'*65,False),(12,False)]:
 candidate=copy.deepcopy(manifest);candidate['composition']['routes']['island']['lexical_context']=selection
 assert validator.is_valid(candidate)==valid,selection
older=copy.deepcopy(manifest);older['composition']['schema_version']=5
assert not validator.is_valid(older)
older['composition']['routes']['island'].pop('lexical_context');assert validator.is_valid(older)
print('Lexical entry schemas:30 version/name/selector cases passed; runtime validates references and expanded quotas')
