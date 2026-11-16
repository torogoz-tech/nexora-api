#!/usr/bin/env python3
"""Check metadata-only local-source PostgreSQL transport acceptance."""
import argparse
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True, type=Path)
    args = parser.parse_args()
    with args.profile.open('rb') as source:
        raw = source.read(5_242_881)
    if len(raw) > 5_242_880:
        parser.error('Profile exceeds5MiB')
    try:
        profile = json.loads(raw)
        if not isinstance(profile, dict):
            raise ValueError('Profile must be an object')
        if (profile['protocol_payloads_or_sql_recorded'] is not False
                or profile['credential_values_recorded'] is not False
                or profile['temporary_api_stopped'] is not True
                or type(profile['temporary_api_exit_code']) is not int
                or profile['temporary_api_exit_code'] != 0
                or profile['proxy_errors'] != {}):
            raise ValueError('Unsafe or unfinished profile')
        measurements = profile['measurements']
        if not isinstance(measurements, list):
            raise ValueError('Measurements must be a list')
        for row in measurements:
            if (not isinstance(row, dict)
                    or type(row.get('sample')) is not int
                    or row['sample'] < 0
                    or type(row.get('warmup_sample', row.get('first_sample'))) is not bool):
                raise ValueError('Invalid sample metadata')
        failures = []
        summary = []
        for operation, executions, maximum in [('catalog', 4, 4), ('overview', 18, 20), ('symbol', 18, 20)]:
            rows = [row for row in measurements if row['operation'] == operation and row.get('warmup_sample', row.get('first_sample')) is False]
            if len(rows) < 8:
                raise ValueError('Incomplete samples')
            for row in rows:
                frames = row['frames']
                if (not isinstance(frames, dict)
                        or not all(isinstance(key, str) and type(value) is int and value >= 0 for key, value in frames.items())):
                    raise ValueError('Invalid frame metadata')
                execute = frames.get('frontend:E', 0)
                close = frames.get('frontend:C', 0)
                ready = frames['backend:Z']
                if execute != executions or close != 0 or not executions <= ready <= maximum:
                    failures.append({'operation': operation, 'sample': row['sample'],
                                     'execute': execute, 'close': close, 'ready': ready, 'maximum_ready': maximum})
            summary.append({'operation': operation, 'samples': len(rows), 'maximum_ready': maximum})
    except (AssertionError, KeyError, TypeError, ValueError):
        print('Invalid or incomplete safe protocol metadata', file=sys.stderr)
        return 2
    print(json.dumps({'passed': not failures, 'operations': summary, 'failures': failures}))
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
