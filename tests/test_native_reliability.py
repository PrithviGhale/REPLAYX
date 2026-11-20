"""Opt-in terminal-only tests of the native encoder and its worker lifetime.
Run with REPLAYX_NATIVE_TEST=1. Uses generated frames, never desktop/audio capture.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).parents[1]
PROTO = ROOT / 'src-tauri/target/release/replayx-native-proto.exe'


@unittest.skipUnless(os.environ.get('REPLAYX_NATIVE_TEST') == '1', 'Opt-in synthetic Intel hardware test')
class NativeReliability(unittest.TestCase):
    vendor = "8086"
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='native-reliability-')
        self.folder = Path(self.temp.name)
        lock = json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'))
        self.ffmpeg = ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'

    def tearDown(self):
        self.temp.cleanup()

    def run_native(self, args, timeout=20):
        import psutil
        # Never wait for exit with undrained PIPEs: diagnostic JSON can exceed
        # Windows' pipe capacity. File-backed capture preserves timeout testing.
        with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
            process = subprocess.Popen([str(PROTO), *map(str,args), "--vendor", self.vendor], stdout=stdout_file, stderr=stderr_file)
            seen = []
            deadline = time.monotonic()+timeout
            try:
                while process.poll() is None:
                    try:
                        seen.extend(psutil.Process(process.pid).children(recursive=True))
                    except psutil.Error:
                        pass
                    if time.monotonic() > deadline:
                        self.fail('Native operation exceeded test deadline')
                    time.sleep(0.02)
            finally:
                if process.poll() is None:
                    process.kill(); process.wait(timeout=5)
            stdout_file.seek(0);stderr_file.seek(0)
            stdout,stderr=stdout_file.read(),stderr_file.read()
        self.assertTrue(all(not p.is_running() for p in seen), 'Worker survived supervisor completion')
        return process.returncode, stdout.decode(), stderr.decode()

    def test_driver_call_watchdog_terminates_worker(self):
        start = time.monotonic()
        code, _, error = self.run_native(['--watchdog-test'])
        self.assertNotEqual(code, 0)
        self.assertIn('shutdown deadline', error)
        self.assertLess(time.monotonic()-start, 5)

    def test_encoder_event_stall_times_out(self):
        start = time.monotonic()
        code, _, error = self.run_native(['--synthetic','regular80',self.folder/'stall.h264','--seconds',2,'--test-encoder-stall'])
        self.assertNotEqual(code, 0)
        self.assertIn('timed out', error)
        self.assertLess(time.monotonic()-start, 12)

    def test_live_input_starvation_fails_before_five_second_offline_budget(self):
        start=time.monotonic()
        code,_,error=self.run_native(['--synthetic','regular80',self.folder/'live-stall.h264','--seconds',2,'--test-encoder-stall','--test-live-input-budget'])
        self.assertNotEqual(code,0)
        self.assertIn('500 ms budget',error)
        self.assertLess(time.monotonic()-start,4)

    def test_delayed_releases_force_safe_surface_reuse(self):
        out = self.folder/'delayed.h264'
        code, stdout, error = self.run_native(['--synthetic','regular80',out,'--seconds',2,'--test-release-delay-ms',50])
        self.assertEqual(code, 0, error)
        report = json.loads(stdout)
        self.assertGreater(report['surface_reuse_waits'],0)
        self.assertEqual(report['live_input_samples'],0)
        self.assertEqual(report['frames_out'], report['encoded'])
        decoded = subprocess.run([str(self.ffmpeg),'-v','error','-i',str(out),'-vf','crop=16:16:952:532,extractplanes=y','-f','rawvideo','-'],capture_output=True,timeout=20,check=True).stdout
        levels = [sum(decoded[i:i+256])/256 for i in range(0,len(decoded),256)]
        self.assertEqual(len(levels),len(report['expected_luma']))
        self.assertTrue(all(abs(actual-expected)<=3 for actual,expected in zip(levels,report['expected_luma'])))

    def test_cancellation_keeps_incremental_output_playable(self):
        out = self.folder/'cancel.h264'
        stop = self.folder/'stop'
        process = subprocess.Popen([str(PROTO),'--synthetic','regular80',str(out),'--seconds','8','--paced','1','--stop-file',str(stop),'--vendor',self.vendor],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic()+6
            while not (out.exists() and out.stat().st_size>0):
                self.assertIsNone(process.poll(), 'Encoder exited before incremental output')
                self.assertLess(time.monotonic(),deadline,'No incremental output before stop')
                time.sleep(0.02)
            start = time.monotonic()
            stop.write_text('stop')
            _, error = process.communicate(timeout=5)
            self.assertNotEqual(process.returncode,0)
            self.assertIn('cancelled',error.decode())
            self.assertLess(time.monotonic()-start,3)
            self.assertGreater(out.stat().st_size,0)
            subprocess.run([str(self.ffmpeg),'-v','error','-xerror','-i',str(out),'-f','null','-'],capture_output=True,timeout=15,check=True)
        finally:
            if process.poll() is None:
                process.kill(); process.communicate(timeout=5)

    def test_repeated_encoder_start_stop_cycles(self):
        for cycle in range(3):
            out = self.folder/f'cycle-{cycle}.h264'
            code, stdout, error = self.run_native(['--feasibility',out,'--frames',120])
            self.assertEqual(code,0,error)
            report = json.loads(stdout)
            self.assertEqual(report['frames_in'],120)
            self.assertEqual(report['frames_out'],120)
            self.assertTrue(report['output_timestamps_increasing'])
            for setting in report['settings_accepted']:
                label, accepted = next(iter(setting.items()))
                if not accepted:
                    self.assertEqual(label, 'b_frames=0')
                    self.assertEqual(report['no_b_frames'], 'validated in every output access unit')
            subprocess.run([str(self.ffmpeg),'-v','error','-xerror','-i',str(out),'-f','null','-'],capture_output=True,timeout=15,check=True)
