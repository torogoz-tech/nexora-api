"""Acceptance-checker regressions using synthetic protocol metadata only."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class PlatformQueryProtocolTests(unittest.TestCase):
    def profile(self):
        return {
            'protocol_payloads_or_sql_recorded': False,
            'credential_values_recorded': False,
            'temporary_api_stopped': True,
            'temporary_api_exit_code': 0,
            'proxy_errors': {},
            'measurements': [
                {'operation': operation, 'sample': index + 2, 'first_sample': False,
                 'warmup_sample': False,
                 'frames': {'frontend:E': execute, 'frontend:C': 0, 'backend:Z': ready}}
                for operation, execute, ready in [('catalog', 4, 4), ('overview', 18, 20), ('symbol', 18, 20)]
                for index in range(8)
            ],
        }

    def check(self, profile, optimized=False):
        with tempfile.TemporaryDirectory(prefix='nexora-protocol-checker-') as directory:
            path = Path(directory) / 'profile.json'
            path.write_text(json.dumps(profile))
            command = [sys.executable]
            if optimized:
                command.append('-O')
            command += [str(Path(__file__).with_name('check-platform-query-protocol.py')), '--profile', str(path)]
            return subprocess.run(command, capture_output=True, text=True, timeout=10).returncode

    def test_complete_profile_passes_with_or_without_python_optimization(self):
        for optimized in [False, True]:
            with self.subTest(optimized=optimized):
                self.assertEqual(self.check(self.profile(), optimized), 0)

    def test_missing_ready_frames_are_rejected(self):
        profile = self.profile()
        for row in profile['measurements']:
            row['frames'].pop('backend:Z')
        self.assertNotEqual(self.check(profile), 0)

    def test_zero_and_inconsistent_ready_frames_are_rejected(self):
        for count in [0, 1, 3]:
            with self.subTest(count=count):
                profile = self.profile()
                for row in profile['measurements']:
                    row['frames']['backend:Z'] = count
                self.assertNotEqual(self.check(profile), 0)

    def test_partial_samples_are_rejected_even_under_optimization(self):
        profile = self.profile()
        profile['measurements'] = profile['measurements'][:1]
        for optimized in [False, True]:
            with self.subTest(optimized=optimized):
                self.assertNotEqual(self.check(profile, optimized), 0)

    def test_unsafe_or_live_profiles_are_rejected_even_under_optimization(self):
        original = self.profile()
        for field, value in [('protocol_payloads_or_sql_recorded', True),
                             ('credential_values_recorded', True),
                             ('temporary_api_stopped', False),
                             ('temporary_api_exit_code', 1),
                             ('proxy_errors', {'FramingError': 1})]:
            for optimized in [False, True]:
                with self.subTest(field=field, optimized=optimized):
                    profile = copy.deepcopy(original)
                    profile[field] = value
                    self.assertNotEqual(self.check(profile, optimized), 0)


if __name__ == '__main__':
    unittest.main()
