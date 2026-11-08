import { test } from 'node:test';
import assert from 'node:assert/strict';
import { audioTrackCount, bytesPerSecond, codecOf, encoderName, ratePlan, requestedCodec, targetBitrate } from '../src/format.ts';
import { micDefaults, micState } from '../src/audio.ts';

// These mirror media::target_bitrate_mbps and media::rate_plan; the expected
// values are the ones asserted by the Rust tests. A stale mirror (16/28/45)
// showed the wrong preset sizes in 0.2.1-alpha.3.
test('preset rates match the engine', () => {
  assert.equal(targetBitrate('performance', 0, 60, 1920, 1080), 10);
  assert.equal(targetBitrate('balanced', 0, 60, 1920, 1080), 16);
  assert.equal(targetBitrate('quality', 0, 60, 1920, 1080), 25);
  assert.equal(targetBitrate('custom', 33, 60, 1920, 1080), 33);
  assert.deepEqual(ratePlan('balanced', 12, 60, 1920, 1080, 'h264'), { target: 16, cap: 24 });
  assert.deepEqual(ratePlan('balanced', 12, 60, 1920, 1080, 'av1'), { target: 12, cap: 18 });
  assert.deepEqual(ratePlan('custom', 20, 60, 1920, 1080, 'av1'), { target: 20, cap: 30 });
});

test('automatic codec is H.264 and encoder ids map to codecs', () => {
  assert.equal(requestedCodec('auto'), 'h264');
  assert.equal(requestedCodec('av1'), 'av1');
  assert.equal(codecOf('av1_nvenc'), 'av1');
  assert.equal(codecOf('hevc_qsv_lp'), 'hevc');
  assert.equal(codecOf('h264_qsv_lp'), 'h264');
  assert.equal(encoderName('av1_nvenc'), 'NVIDIA NVENC');
  assert.equal(encoderName('hevc_qsv_lp'), 'Intel Quick Sync (low-power)');
});

test('storage estimate counts every audio track', () => {
  assert.equal(audioTrackCount(0, true), 0);
  assert.equal(audioTrackCount(3, true), 4);
  assert.equal(audioTrackCount(3, false), 1);
  // 16 Mb/s video + 4 x 192 kb/s AAC, +3% container: about 130 MB per minute.
  const perMinute = bytesPerSecond(16, 4, 192) * 60;
  assert.ok(perMinute > 125e6 && perMinute < 135e6, String(perMinute));
});

test('microphone state never claims recording unless capture runs', () => {
  const mic = micDefaults('');
  assert.equal(mic.high_pass, true);
  assert.equal(mic.boost_db, 0);
  assert.equal(micState({ mic: null, capturing: true, deviceKnown: true }).key, 'none');
  assert.equal(micState({ mic, capturing: false, deviceKnown: true }).key, 'ready');
  assert.equal(micState({ mic, capturing: true, deviceKnown: true }).key, 'recording');
  assert.equal(micState({ mic: { ...mic, muted: true }, capturing: true, deviceKnown: true }).key, 'muted');
  assert.equal(micState({ mic: { ...mic, enabled: false }, capturing: true, deviceKnown: true }).key, 'off');
  assert.equal(micState({ mic, capturing: true, deviceKnown: false }).key, 'missing');
});
