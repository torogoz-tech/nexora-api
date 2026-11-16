# /// script
# dependencies = ["psutil>=7,<8"]
# ///
"""Measure unchanged-repository CLI refreshes sequentially, including startup and output."""
import json,subprocess,tempfile,time,statistics
from pathlib import Path
import psutil
base=Path(__file__).resolve().parents[2]/'.nexora/local/benchmarks/smartticket';workspace=base.parents[3];settings=json.loads((base/'settings.json').read_text())
results={}
for backend in ['nexora','serena']:
 samples=[]
 for iteration in range(4):
  command=[str(workspace/'target/release/nexora'),'--language-dir',str(base/'profile'),'index',str(base/'source'),'--tenant','benchmark-smartticket-v1','--report'] if backend=='nexora' else [settings['serena_command'],'project','index',str(base/'source'),'--log-level','WARNING','--timeout','30']
  start=time.perf_counter();peak=0
  with tempfile.TemporaryFile() as output,tempfile.TemporaryFile() as error:
   process=subprocess.Popen(command,stdout=output,stderr=error)
   while process.poll() is None:
    try:peak=max(peak,sum(p.memory_info().rss for p in [psutil.Process(process.pid)]+psutil.Process(process.pid).children(recursive=True) if p.is_running()))
    except psutil.Error:pass
    if time.perf_counter()-start>180:process.kill();process.wait();break
    time.sleep(.02)
   wall=(time.perf_counter()-start)*1000
   row={'exit_code':process.returncode,'wall_ms':wall,'sampled_process_tree_peak_rss_bytes':peak}
   if backend=='nexora' and process.returncode==0:
    output.seek(0);r=json.load(output);row.update(reuse=r['reuse'],operation_ms=r['operation_ms'],storage_ms=r['storage_ms'],indexer_ms=r['timings']['total_ms'])
   if iteration:samples.append(row)
 results[backend]={'priming_runs_excluded':1,'measured_runs':samples,'median_wall_ms':statistics.median(r['wall_ms'] for r in samples)}
 (base/'warm-index-comparison.json').write_text(json.dumps(results,indent=2));print(json.dumps({'backend':backend,'median_wall_ms':results[backend]['median_wall_ms'],'exit_codes':[r['exit_code'] for r in samples]}),flush=True)
