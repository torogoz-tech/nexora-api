#!/usr/bin/env python3
"""Isolated-process benchmark of trusted Nexora CLI; corpus contains data only."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time


def percentile(values, probability):
    if not values:
        raise ValueError('empty sample')
    return sorted(values)[max(0, math.ceil(len(values) * probability) - 1)]


def summary(values):
    return dict(median=statistics.median(values), p95=percentile(values, .95), min=min(values), max=max(values))


def worker(command, timeout):
    started = time.perf_counter()
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        usage = None
        try:
            if hasattr(os, 'wait4'):
                while True:
                    pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                    if pid:
                        process.returncode = os.waitstatus_to_exitcode(status)
                        break
                    if time.perf_counter() - started > timeout:
                        raise subprocess.TimeoutExpired(command, timeout)
                    time.sleep(.002)
            else:
                process.wait(timeout=timeout)
        except BaseException:
            process.kill()
            process.wait()
            raise
        elapsed = (time.perf_counter() - started) * 1000
        stdout.seek(0)
        stderr.seek(0)
        if process.returncode:
            raise ValueError(f'worker failed ({process.returncode}): {stderr.read(4096).decode(errors="replace")}')
        report = json.load(stdout)
    rss = None
    reason = 'wait4 unavailable or unsupported RSS units'
    if usage is not None and sys.platform in ('darwin', 'linux'):
        rss = usage.ru_maxrss * (1 if sys.platform == 'darwin' else 1024)
        reason = None
    return report, dict(wall_ms=elapsed, peak_process_rss_bytes=rss, memory_unavailable=reason,
                        cpu_user_ms=usage.ru_utime * 1000 if usage else None,
                        cpu_system_ms=usage.ru_stime * 1000 if usage else None)


def validate_report(report, case_id=None, fingerprint=None):
    if not report.get('passed') or not report.get('cases') or not all(c['passed'] for c in report['cases']):
        raise ValueError('correctness failed; performance is invalid')
    if case_id is not None:
        if len(report['cases']) != 1 or report['cases'][0]['id'] != case_id or report['cases'][0]['fingerprint'] != fingerprint:
            raise ValueError('worker case/fingerprint mismatch')


def compare_baseline(current, baseline, time_limit=None, memory_limit=None):
    if not isinstance(baseline, dict):
        raise ValueError('baseline must be a report object')
    for field in ('schema_version', 'corpus_fingerprint', 'environment', 'build', 'iterations', 'warmup'):
        if current[field] != baseline[field]:
            raise ValueError(f'incompatible baseline: {field}')
    if not baseline.get('correctness_passed') or not baseline.get('cases'):
        raise ValueError('baseline lacks validated correctness/cases')
    if [c['id'] for c in current['cases']] != [c['id'] for c in baseline['cases']]:
        raise ValueError('incompatible baseline: selected cases')
    comparisons = []
    for case, previous in zip(current['cases'], baseline['cases']):
        if case['fingerprint'] != previous['fingerprint'] or case['source_bytes'] != previous['source_bytes']:
            raise ValueError('incompatible case input')
        changes = {}
        for metric, threshold in [('wall_ms', time_limit), ('peak_process_rss_bytes', memory_limit)]:
            old = previous['metrics'].get(metric)
            new = case['metrics'].get(metric)
            if old is None or new is None:
                if threshold is not None:
                    raise ValueError(f'required baseline metric unavailable: {metric}')
                changes[metric] = None
                continue
            before, after = old['median'], new['median']
            if not math.isfinite(before) or not math.isfinite(after) or before <= 0 or after < 0:
                raise ValueError('invalid baseline metric')
            percentage = 100 * (after / before - 1)
            changes[metric] = dict(percent=percentage, threshold=threshold,
                                   exceeded=threshold is not None and percentage > threshold)
        comparisons.append(dict(id=case['id'], changes=changes))
    return comparisons


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/nexora'))
    parser.add_argument('--corpus', default='corpus/suite.yaml')
    parser.add_argument('--workspace', type=Path, default=Path('.'))
    parser.add_argument('--iterations', type=int, default=5)
    parser.add_argument('--warmup', type=int, default=1)
    parser.add_argument('--timeout', type=float, default=60)
    parser.add_argument('--case', action='append')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--max-time-regression-percent', type=float)
    parser.add_argument('--max-memory-regression-percent', type=float)
    parser.add_argument('--require-memory', action='store_true')
    args = parser.parse_args()
    if not 1 <= args.iterations <= 100 or not 0 <= args.warmup <= 20 or not 0 < args.timeout <= 60:
        raise ValueError('iterations 1..100, warmup 0..20, timeout (0,60] required')
    limits = (args.max_time_regression_percent, args.max_memory_regression_percent)
    if any(v is not None and (not math.isfinite(v) or v < 0) for v in limits):
        raise ValueError('finite nonnegative thresholds required')
    if any(v is not None for v in limits) and not args.baseline:
        raise ValueError('thresholds require baseline')
    binary = args.binary.resolve(strict=True)
    command = [str(binary), 'corpus']
    common = [args.corpus, '--workspace', str(args.workspace.resolve(strict=True))]
    description, _ = worker(command + ['validate'] + common, args.timeout)
    correctness, _ = worker(command + ['check'] + common, args.timeout)
    validate_report(correctness)
    descriptors = description['cases']
    selected = sorted(args.case) if args.case else sorted(c['id'] for c in descriptors if c['benchmark'])
    if not selected or len(selected) != len(set(selected)) or set(selected) - {c['id'] for c in descriptors}:
        raise ValueError('unknown, duplicated, or empty benchmark selection')
    descriptor_by_id = {c['id']: c for c in descriptors}
    report = dict(schema_version=1, corpus_fingerprint=description['fingerprint'], correctness_passed=True,
                  analysis_engine=correctness['analysis_engine'], governance_engine=correctness['governance_engine'],
                  binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), build=correctness['build'],
                  iterations=args.iterations, warmup=args.warmup,
                  environment=dict(os=platform.system(), os_release=platform.release(), architecture=platform.machine(),
                                   processor=platform.processor(), logical_cpus=os.cpu_count(),
                                   host_sha256=hashlib.sha256(platform.node().encode()).hexdigest()), cases=[])
    for case_id in selected:
        samples = []
        for iteration in range(args.warmup + args.iterations):
            result, sample = worker(command + ['check'] + common + ['--case', case_id], args.timeout)
            validate_report(result, case_id, descriptor_by_id[case_id]['fingerprint'])
            if result['corpus_fingerprint'] != description['fingerprint']:
                raise ValueError('corpus changed during benchmark')
            if args.require_memory and sample['peak_process_rss_bytes'] is None:
                raise ValueError('required peak RSS unavailable')
            case = result['cases'][0]
            if case['pipeline'] is None:
                raise ValueError('benchmark requires successful measured analysis')
            sample.update(loading_ms=result['loading_ms'], pipeline=case['pipeline'], setup_ms=case['setup_ms'], analysis_elapsed_ms=case['analysis_elapsed_ms'],
                          policy_ms=case['policy_ms'], assertions_ms=case['assertions_ms'], total_ms=case['total_ms'])
            if iteration >= args.warmup:
                samples.append(sample)
        metrics = {}
        for name in ('wall_ms', 'peak_process_rss_bytes', 'cpu_user_ms', 'cpu_system_ms', 'setup_ms',
                     'analysis_elapsed_ms', 'policy_ms', 'assertions_ms', 'total_ms', 'loading_ms'):
            values = [s[name] for s in samples]
            metrics[name] = summary(values) if all(v is not None for v in values) else None
        stages = {name: summary([s['pipeline'][name] for s in samples]) for name in samples[0]['pipeline']}
        analysis_ms = metrics['analysis_elapsed_ms']['median']
        report['cases'].append(dict(id=case_id, fingerprint=case['fingerprint'], source_bytes=case['source_bytes'],
                                    metrics=metrics, stages=stages, samples=samples,
                                    analysis_bytes_per_second=case['source_bytes'] * 1000 / analysis_ms if analysis_ms > 0 else None))
    comparisons = compare_baseline(report, json.loads(args.baseline.read_text()), *limits) if args.baseline else []
    report['comparison'] = comparisons
    report['regression_passed'] = not any(change and change['exceeded'] for c in comparisons for change in c['changes'].values())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', dir=args.output.parent, delete=False) as output:
        json.dump(report, output, indent=2, allow_nan=False)
        output.write('\n')
        temporary = output.name
    os.replace(temporary, args.output)
    print(f'{len(selected)} cases; correctness passed; performance saved to {args.output}')
    return 0 if report['regression_passed'] else 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        print(f'benchmark: {error}', file=sys.stderr)
        sys.exit(2)
