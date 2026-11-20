"""Exercise the benchmark's complete control flow with a virtual clock and fake processes."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

BENCH = Path(__file__).parents[1] / 'scripts/bench'


class ScheduledSession(unittest.TestCase):
    def test_complete_three_pair_run_obeys_reserved_schedule(self):
        sys.path.insert(0, str(BENCH))
        try:
            spec = importlib.util.spec_from_file_location('session_under_test', BENCH/'fortnite-session.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        clock = [0.0]
        processes = []
        def advance(seconds):
            clock[0] += max(0, seconds)
        class Process:
            def __init__(self, command, **options):
                self.pid = 1000 + len(processes)
                self.returncode = None
                self.stdout = []
                self.end = float('inf')
                processes.append(self)
                if '--record' in command:
                    self.end = clock[0]+int(command[command.index('--seconds')+1])
                    json.dump(dict(success=True,frames_out=2520,dup_frames=0), options['stdout'])
                    options['stdout'].flush()
                elif 'capture-test' in command:
                    if 'wall_clock=1' not in command or 'wait_for_replay=1' not in command:
                        raise AssertionError('Benchmark capture duration must include initialization')
                    self.end = clock[0]+int(command[2])
                    report = Path(options['cwd'])/'test-output/capture-test-fixture/report.json'
                    report.parent.mkdir(parents=True)
                    report.write_text(json.dumps(dict(status=dict(encoder='h264_qsv_lp',fps=60),wall_clock_budget=True,requested_seconds=int(command[2]),capture_end_ms=int(command[2])*1000)))
                elif '--timed' in command:
                    self.end = clock[0]+int(command[command.index('--timed')+1])
                    Path(command[command.index('--output_file')+1]).touch()
            def poll(self):
                if clock[0] >= self.end:
                    self.returncode = 0
                return self.returncode
            def terminate(self):
                self.returncode = 0
            def wait(self, timeout):
                if self.poll() is None:
                    raise AssertionError('Unexpected outstanding fixture process')
                return self.returncode
        game = SimpleNamespace(pid=123, name=lambda:'cs2.exe', is_running=lambda:True)
        configs = 'wgc-auto,native-record,native-record,wgc-auto,wgc-auto,native-record'
        with tempfile.TemporaryDirectory() as temporary, contextlib.ExitStack() as stack:
            root = Path(temporary)
            build = root/'build'
            (build/'media').mkdir(parents=True)
            (root/'tools').mkdir()
            (root/'tools/ffmpeg-lock.json').write_text('{"runtime_dir":"pinned/bin"}', encoding='utf-8')
            for name in ['replayx-cli.exe','replayx-native-proto.exe','media/ffmpeg.exe','media/ffprobe.exe']:
                (build/name).write_bytes(b'fixture')
            patches = [patch.object(module,'ROOT',root), patch.object(module,'find_game',return_value=game),
                       patch.object(module,'game_in_front',return_value=True),
                       patch.object(module.psutil,'process_iter',return_value=[]),
                       patch.object(module.ctypes.windll.shell32,'IsUserAnAdmin',return_value=True),
                       patch.object(module.subprocess,'Popen',Process),
                       patch.object(module.subprocess,'run',return_value=SimpleNamespace(stdout='fixture revision')),
                       patch.object(module.threading,'Thread'),
                       patch.object(module.threading,'Event',return_value=SimpleNamespace(wait=advance)),
                       patch.object(module.time,'monotonic',side_effect=lambda:clock[0]),
                       patch.object(module.time,'time',side_effect=lambda:1_700_000_000+clock[0]),
                       patch.object(module.time,'sleep',side_effect=advance),
                       patch.object(module.frametimes,'summarize',return_value=dict(fps=300,low1_fps=200,p99_ms=5)),
                       patch.object(module,'present_modes',return_value={}),
                       patch.object(sys,'argv',['session','--game','cs2','--build',str(build),'--seconds','30',
                                                '--warmup','60','--configs',configs,'--presentmon',str(root/'presentmon.exe')])]
            for replacement in patches:
                stack.enter_context(replacement)
            output = io.StringIO()
            with contextlib.redirect_stdout(output), self.assertRaises(SystemExit) as exit_result:
                module.main()
            self.assertEqual(exit_result.exception.code,0)
            result = json.loads(next((root/'test-output').rglob('summary.json')).read_text())
            self.assertEqual(result['errors'],[])
            self.assertEqual(result['timing']['completed_phases'],13)
            self.assertEqual(result['timing']['elapsed_seconds'],703)
            self.assertEqual(result['paired_costs']['pairs'],3)
            self.assertIn('DONE after 11:43',output.getvalue())
            self.assertTrue(all(p.poll() is not None for p in processes))


class RetainedNativeReport(unittest.TestCase):
    def test_report_survives_session_cleanup_and_rejects_other_profiles(self):
        sys.path.insert(0, str(BENCH))
        try:
            spec = importlib.util.spec_from_file_location('retained_report_session', BENCH/'fortnite-session.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            phase = root/'phase'
            session = phase/'session'
            session.mkdir(parents=True)
            original = session/'native.json'
            original.write_text('{"complete":true}')
            module.retain_native_report([['worker','--origin-file',str(root/'unrelated'/'origin.json')]],phase)
            self.assertFalse((phase/'native-report.json').exists())
            module.retain_native_report([['worker','--origin-file',str(session/'origin.json')]],phase)
            original.unlink()
            self.assertEqual(json.loads((phase/'native-report.json').read_text()),{'complete':True})

            (phase/'native-report.json').unlink()
            commands = {}
            calls = []

            def show_delayed_worker():
                calls.append('refresh')
                original.write_text('{"complete":false}')
                commands[42] = ['worker', '--origin-file', str(session/'origin.json')]

            module.settle_native_report(commands, phase, show_delayed_worker, lambda: calls.append('wait'))
            self.assertEqual(calls, ['wait', 'refresh'])
            original.write_text('{"complete":true}')
            original.unlink()
            self.assertTrue(json.loads((phase/'native-report.json').read_text())['complete'])
            module.settle_native_report(commands, phase, show_delayed_worker, lambda: calls.append('extra wait'))
            self.assertEqual(calls, ['wait', 'refresh'])


class NativeCompletion(unittest.TestCase):
    def test_requires_frames_released_surfaces_and_the_requested_interval(self):
        sys.path.insert(0, str(BENCH))
        try:
            spec = importlib.util.spec_from_file_location('completion_session', BENCH/'fortnite-session.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        complete = dict(success=True, frames_out=600, encoded=600, live_input_samples=0,
                        output_timestamp_contract_valid=True, min_update_interval_us=6666,
                        min_update_interval_accepted=True)
        module.validate_native_report(complete, 6666)
        for change in [dict(frames_out=0, encoded=0), dict(encoded=599),
                       dict(live_input_samples=1), dict(success=False),
                       dict(output_timestamp_contract_valid=False),
                       dict(min_update_interval_us=0), dict(min_update_interval_accepted=False)]:
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                module.validate_native_report(dict(complete, **change), 6666)
        module.validate_native_report(complete, None)
        self.assertEqual(module.CONFIGS['app-ffmpeg-matched-steady'][1], 'wgc_oversample=2')
        self.assertEqual(module.CONFIGS['app-ffmpeg-nvenc-fixed'][1], '')
        module.validate_capture_continuity(dict(recovery_sessions=0))
        with self.assertRaises(RuntimeError):
            module.validate_capture_continuity(dict(recovery_sessions=1))
