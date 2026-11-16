import importlib.util
from pathlib import Path
import sys
import unittest
spec = importlib.util.spec_from_file_location('benchmark', Path(__file__).with_name('benchmark-corpus.py'))
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class BenchmarkTests(unittest.TestCase):
    def baseline(self):
        return dict(schema_version=1, corpus_fingerprint='data', environment={}, build='release', iterations=5, warmup=1,
                    correctness_passed=True, cases=[dict(id='case', fingerprint='facts', source_bytes=100,
                    metrics=dict(wall_ms=dict(median=100), peak_process_rss_bytes=dict(median=1000)))])

    def test_nearest_rank_p95_and_median(self):
        self.assertEqual(benchmark.percentile([1, 2, 3, 4, 5], .95), 5)
        self.assertEqual(benchmark.summary([1, 2, 9])['median'], 2)

    def test_regression_direction_and_threshold(self):
        current, baseline = self.baseline(), self.baseline()
        current['cases'][0]['metrics']['wall_ms']['median'] = 120
        result = benchmark.compare_baseline(current, baseline, 10, 10)
        self.assertTrue(result[0]['changes']['wall_ms']['exceeded'])
        self.assertFalse(result[0]['changes']['peak_process_rss_bytes']['exceeded'])

    def test_incompatible_data_and_unvalidated_baselines_rejected(self):
        for field, value in [('corpus_fingerprint', 'other'), ('environment', {'os': 'other'}), ('correctness_passed', False)]:
            old = self.baseline()
            old[field] = value
            with self.assertRaises(ValueError):
                benchmark.compare_baseline(self.baseline(), old)

    def test_malformed_baseline_is_configuration_error(self):
        with self.assertRaises(ValueError):
            benchmark.compare_baseline(self.baseline(), [])

    def test_zero_and_missing_memory_not_faked(self):
        old = self.baseline()
        old['cases'][0]['metrics']['wall_ms']['median'] = 0
        with self.assertRaises(ValueError):
            benchmark.compare_baseline(self.baseline(), old)
        old = self.baseline()
        old['cases'][0]['metrics']['peak_process_rss_bytes'] = None
        self.assertIsNone(benchmark.compare_baseline(self.baseline(), old)[0]['changes']['peak_process_rss_bytes'])
        with self.assertRaises(ValueError):
            benchmark.compare_baseline(self.baseline(), old, memory_limit=10)

    def test_warmup_failure_and_case_mismatch_are_errors(self):
        for report in [dict(passed=False, cases=[]), dict(passed=True, cases=[dict(passed=False)])]:
            with self.assertRaises(ValueError):
                benchmark.validate_report(report)
        with self.assertRaises(ValueError):
            benchmark.validate_report(dict(passed=True, cases=[dict(passed=True, id='a', fingerprint='x')]), 'b', 'x')

    def test_worker_failure_and_timeout_are_errors(self):
        with self.assertRaises(ValueError):
            benchmark.worker([sys.executable, '-c', 'raise SystemExit(1)'], 1)
        with self.assertRaises(benchmark.subprocess.TimeoutExpired):
            benchmark.worker([sys.executable, '-c', 'import time; time.sleep(1)'], .02)


if __name__ == '__main__':
    unittest.main()
