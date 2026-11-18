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

    def run_codex_prompt(self, root, event, with_helper=True, action='prompt'):
        helper = root / 'helper.py'
        helper.write_text('import os,sys\nfrom pathlib import Path\nPath(os.environ["SYNTHETIC_CAPTURE"]).write_bytes(sys.stdin.buffer.read())\n')
        capture = root / 'prompt.json'
        env = {**os.environ, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'synthetic',
               'HERDR_SOCKET_PATH': str(root / 'absent.socket'),
               'PRETORIAN_HISTORICOS_PYTHON': sys.executable,
               'PRETORIAN_HISTORICOS_HELPER': str(helper), 'SYNTHETIC_CAPTURE': str(capture)}
        if not with_helper:
            env.pop('PRETORIAN_HISTORICOS_HELPER')
        script = ROOT / 'src/integration/assets/codex/herdr-agent-state.sh'
        result = subprocess.run(['sh', str(script), action], input=json.dumps(event).encode(),
                                env=env, capture_output=True, timeout=3)
        return result, capture

    def test_codex_prompt_delivers_reduced_event_without_output(self):
        event = {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'turn_id': 'turn',
                 'transcript_path': '/synthetic.jsonl', 'cwd': '/synthetic', 'model': 'm',
                 'prompt': 'á' * 100000}
        with tempfile.TemporaryDirectory() as temporary:
            result, capture = self.run_codex_prompt(Path(temporary), event)
            self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
            self.assertEqual(json.loads(capture.read_bytes()),
                             {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'prompt': 'á' * 2000})

    def test_codex_prompt_keeps_short_prompt_whole(self):
        event = {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'prompt': 'peticionar'}
        with tempfile.TemporaryDirectory() as temporary:
            result, capture = self.run_codex_prompt(Path(temporary), event)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(json.loads(capture.read_bytes()),
                             {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'prompt': 'peticionar'})

    def test_codex_prompt_without_helper_calls_nothing(self):
        event = {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'prompt': 'x'}
        with tempfile.TemporaryDirectory() as temporary:
            result, capture = self.run_codex_prompt(Path(temporary), event, with_helper=False)
            self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
            self.assertFalse(capture.exists())

    def test_codex_prompt_ignores_other_events_and_missing_fields(self):
        events = [{'hook_event_name': 'SessionStart', 'session_id': 'synthetic', 'prompt': 'x'},
                  {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic'},
                  {'hook_event_name': 'UserPromptSubmit', 'prompt': 'x'}]
        for event in events:
            with self.subTest(event=event), tempfile.TemporaryDirectory() as temporary:
                result, capture = self.run_codex_prompt(Path(temporary), event)
                self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
                self.assertFalse(capture.exists())

    def test_codex_session_action_ignores_prompt_event(self):
        event = {'hook_event_name': 'UserPromptSubmit', 'session_id': 'synthetic', 'prompt': 'x'}
        with tempfile.TemporaryDirectory() as temporary:
            result, capture = self.run_codex_prompt(Path(temporary), event, action='session')
            self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'', b''))
            self.assertFalse(capture.exists())


if __name__ == '__main__':
    unittest.main()
