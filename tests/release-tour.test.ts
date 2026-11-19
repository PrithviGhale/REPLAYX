import assert from 'node:assert/strict';
import { test } from 'node:test';
import { advanceTourState, compareVersions, featuresFor, pendingTour, readTourState, RELEASES, RELEASES_PAGE, releaseNotesUrl } from '../src/release-tour.ts';
import { DEFAULT_ROUTE, PAGES, WORKSPACES, parseRoute, routeHash } from '../src/workspace.ts';

test('workspace URLs round trip and legacy tray destinations still resolve', () => {
  assert.equal(new Set(PAGES.map(page => page.id)).size, PAGES.length);
  for (const workspace of WORKSPACES) for (const page of workspace.pages) {
    assert.deepEqual(parseRoute(routeHash({ ...DEFAULT_ROUTE, page: page.id })), { ...DEFAULT_ROUTE, page: page.id });
  }
  assert.deepEqual(parseRoute('#settings:updates'), { page: 'settings', tab: 'updates' });
  assert.deepEqual(parseRoute('settings:hotkeys'), { page: 'settings', tab: 'hotkeys' });
  assert.equal(parseRoute('#clips:updates'), null);
  assert.equal(parseRoute('#not-a-page'), null);
});

test('fresh installs can defer the welcome; old settings never trigger it', () => {
  const first = advanceTourState(null, '0.4.0', 'fresh', false);
  assert.equal(pendingTour(first), 'welcome');
  assert.equal(advanceTourState(first, '0.4.0', 'existing', false), first);
  assert.equal(pendingTour({ ...first, outcomes: { welcome: 'dismissed' } }), null);
  assert.equal(pendingTour({ ...first, outcomes: { welcome: 'completed' } }), null);
  assert.equal(pendingTour(advanceTourState(null, '0.4.0', 'fresh', true)), null);
  assert.equal(pendingTour(advanceTourState(null, '0.4.0', 'existing', false)), 'update');
});

test('Alpha.9 and 0.4 prereleases see only their new features', () => {
  const alpha = advanceTourState(null, '0.4.0', 'pre-0.4', true);
  const names = (previous: string | null) => featuresFor(RELEASES[0], previous).map(feature => feature.id);
  assert.equal(pendingTour(alpha), 'update');
  assert.deepEqual(names(alpha.previous), ['workspaces', 'crosshair', 'native-engine']);
  assert.deepEqual(names('0.4.0-rc.1'), ['workspaces']);
  assert.deepEqual(names('0.4.0'), []);
  assert.equal(compareVersions('0.4.0-rc.2', '0.4.0-rc.10'), -1);
  assert.equal(compareVersions('0.4.0', '0.4.0-rc.10'), 1);
  assert.equal(compareVersions('0.4.1', '0.4.0'), 1);
});

test('a fix-only release advances without tour steps and preserves history', () => {
  const previous = advanceTourState(null, '0.4.0', 'fresh', false);
  previous.outcomes.welcome = 'completed';
  const manifest = [{ version: '0.4.1', features: [], improvements: [], fixes: ['Sample'] }];
  const next = advanceTourState(previous, '0.4.1', 'existing', false);
  assert.equal(pendingTour(next, manifest), 'update');
  assert.deepEqual(featuresFor(manifest[0], next.previous), []);
  assert.equal(next.outcomes.welcome, 'completed');
  next.outcomes['release:0.4.1'] = 'dismissed';
  assert.equal(pendingTour(readTourState(JSON.stringify(next))!, manifest), null);
  const downgrade = advanceTourState(next, '0.4.0', 'existing', false);
  assert.equal(pendingTour(downgrade), null);
});

test('malformed preferences and unsafe release tag strings fall back safely', () => {
  assert.equal(readTourState('{broken'), null);
  assert.equal(readTourState('{"version":"invalid"}'), null);
  assert.deepEqual(readTourState('{"version":"0.4.0","outcomes":{"welcome":"bad","release:../../x":"completed"}}')?.outcomes, {});
  assert.equal(releaseNotesUrl('0.4.0'), RELEASES_PAGE + '/tag/v0.4.0');
  assert.equal(releaseNotesUrl('0.4.0-rc.3'), RELEASES_PAGE + '/tag/v0.4.0-rc.3');
  for (const version of ['../../bad', '0.4.0?x=abc', 'https://site.example', '']) assert.equal(releaseNotesUrl(version), RELEASES_PAGE);
});
