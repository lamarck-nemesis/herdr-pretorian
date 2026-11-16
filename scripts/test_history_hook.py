import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class HistoryHook(unittest.TestCase):
    def test_managed_session_reports_once_without_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helper = root / 'helper.py'
            helper.write_text('import os,sys\nfrom pathlib import Path\nPath(os.environ["SYNTHETIC_CAPTURE"]).write_bytes(sys.stdin.buffer.read())\n')
            for agent in ('codex', 'claude'):
                with self.subTest(agent=agent):
                    capture = root / (agent + '.json')
                    env = {**os.environ, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'synthetic',
                           'HERDR_SOCKET_PATH': str(root / 'absent.socket'),
                           'PRETORIAN_HISTORICOS_PYTHON': sys.executable,
                           'PRETORIAN_HISTORICOS_HELPER': str(helper), 'PRETORIAN_HISTORICOS_CONTEXTO': '{}',
                           'SYNTHETIC_CAPTURE': str(capture)}
                    event = {'hook_event_name': 'SessionStart', 'session_id': 'synthetic', 'source': 'startup',
                             'transcript_path': str(root / 'synthetic.jsonl')}
                    script = ROOT / 'src/integration/assets' / agent / 'herdr-agent-state.sh'
                    result = subprocess.run(['sh', str(script), 'session'], input=json.dumps(event).encode(),
                                            env=env, capture_output=True, timeout=3)
                    self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
                    self.assertEqual(json.loads(capture.read_bytes()), event)
                    capture.unlink()
                    env.pop('PRETORIAN_HISTORICOS_HELPER')
                    result = subprocess.run(['sh', str(script), 'session'], input=json.dumps(event).encode(),
                                            env=env, capture_output=True, timeout=3)
                    self.assertEqual(result.returncode, 0)
                    self.assertFalse(capture.exists())

    def test_failed_or_slow_recorder_does_not_break_agent_start(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper = Path(temporary) / 'helper.py'
            helper.write_text('import time\ntime.sleep(10)\n')
            for agent in ('codex', 'claude'):
                with self.subTest(agent=agent):
                    env = {**os.environ, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'synthetic',
                           'HERDR_SOCKET_PATH': str(Path(temporary) / 'absent.socket'),
                           'PRETORIAN_HISTORICOS_PYTHON': sys.executable, 'PRETORIAN_HISTORICOS_HELPER': str(helper),
                           'PRETORIAN_HISTORICOS_CONTEXTO': '{}'}
                    event = {'hook_event_name': 'SessionStart', 'session_id': 'synthetic', 'transcript_path': '/synthetic.jsonl'}
                    started = time.monotonic()
                    result = subprocess.run(['sh', str(ROOT / 'src/integration/assets' / agent / 'herdr-agent-state.sh'), 'session'],
                                            input=json.dumps(event).encode(), env=env, capture_output=True, timeout=3)
                    self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
                    self.assertLess(time.monotonic() - started, 2)


if __name__ == '__main__':
    unittest.main()
