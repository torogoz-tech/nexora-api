# /// script
# dependencies = ["mcp==2.3.0", "psutil>=7,<8"]
# ///
"""Regrade retained raw responses after normalizing FastMCP's result wrapper; timings unchanged."""
import importlib.util,json
from pathlib import Path
spec=importlib.util.spec_from_file_location('compare_queries',Path(__file__).with_name('compare_queries.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
bench=module.ComparisonBenchmark();report=json.loads((bench.base/'query-comparison.json').read_text())
for backend,data in report['backends'].items():
 for row in data['queries']:
  checks=[]
  for iteration in range(1,6):
   raw=json.loads((bench.base/f'{backend}-case-{row["case"]}-{row["operation"]}-{iteration}.json').read_text())
   _,value=bench.decode(raw);checks.append(bench.correctness(backend,row['operation'],value,bench.cases[row['case']]))
  row['correctness']=checks;row['all_correct']=all(c['passed'] for c in checks)
 print(json.dumps({'backend':backend,'correct_cases':{operation:sum(row['all_correct'] for row in data['queries'] if row['operation']==operation) for operation in ['definition','context','search']}}))
report['grading_note']='Canonical content text decoded for both tools. Serena FastMCP structuredContent wraps its string in result; the initial harness treated that wrapper as semantic data. Raw results were regraded; latency samples are unchanged.'
(bench.base/'query-comparison.json').write_text(json.dumps(report,indent=2))
