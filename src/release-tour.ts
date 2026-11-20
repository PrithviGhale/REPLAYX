export const TOUR_STORAGE = 'replayx.tour.v1';
export const RELEASES_PAGE = 'https://github.com/PrithviGhale/REPLAYX/releases';

export type InstallOrigin = 'fresh' | 'pre-0.4' | 'existing';
export type Outcome = 'completed' | 'dismissed';
export type TourState = {
  version: string;
  welcome: boolean;
  previous: string | null;
  outcomes: Record<string, Outcome>;
};

type Feature = { id: string; since: string; title: string; summary: string };
export type Release = {
  version: string;
  features: readonly Feature[];
  improvements: readonly string[];
  fixes: readonly string[];
};

export const RELEASES: readonly Release[] = [{
  version: '0.4.0',
  features: [
    { id: 'workspaces', since: '0.4.0', title: 'A new capture workspace', summary: 'Find capture, clips, studio and settings in separate workspaces. Record and save from any page.' },
    { id: 'crosshair', since: '0.4.0-rc.1', title: 'Crosshair Studio', summary: 'Customize an on-screen crosshair for supported windowed or borderless games.' },
    { id: 'native-engine', since: '0.4.0-rc.1', title: 'Experimental Native Engine', summary: 'Try the optional native recorder on supported graphics hardware, with FFmpeg fallback.' },
  ],
  improvements: ['Faster clip saving with the same recording quality.'],
  fixes: ['Corrected audio timing across recording and replay segments.'],
}];

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
const parse = (value: string) => value.length <= 64 ? SEMVER.exec(value) : null;

export function compareVersions(left: string, right: string): number {
  const a = parse(left);
  const b = parse(right);
  if (!a || !b) return 0;
  for (let i = 1; i <= 3; ++i) {
    const result = a[i].length - b[i].length || a[i].localeCompare(b[i], 'en');
    if (result) return Math.sign(result);
  }
  if (!a[4] || !b[4]) return Number(!a[4]) - Number(!b[4]);
  const x = a[4].split('.');
  const y = b[4].split('.');
  for (let i = 0; i < Math.min(x.length, y.length); ++i) {
    if (x[i] === y[i]) continue;
    const nx = /^\d+$/.test(x[i]);
    const ny = /^\d+$/.test(y[i]);
    if (nx !== ny) return nx ? -1 : 1;
    return Math.sign((nx ? x[i].length - y[i].length : 0) || (x[i] < y[i] ? -1 : 1));
  }
  return Math.sign(x.length - y.length);
}

export function releaseNotesUrl(version: string) {
  return parse(version) ? RELEASES_PAGE + '/tag/v' + version : RELEASES_PAGE;
}

export function readTourState(text: string | null): TourState | null {
  try {
    const raw = JSON.parse(text ?? 'null') as TourState | null;
    if (!raw || typeof raw.version !== 'string' || !parse(raw.version)) return null;
    const previous = typeof raw.previous === 'string' && parse(raw.previous) ? raw.previous : null;
    const outcomes = Object.fromEntries(Object.entries(raw.outcomes ?? {}).filter(([key, value]) =>
      (key === 'welcome' || (key.startsWith('release:') && parse(key.slice(8)))) &&
      (value === 'completed' || value === 'dismissed')));
    return { version: raw.version, previous, welcome: raw.welcome === true, outcomes };
  } catch {
    return null;
  }
}

export function advanceTourState(state: TourState | null, version: string, origin: InstallOrigin, oldTourDone: boolean): TourState {
  if (state?.version === version) return state;
  if (state) {
    const previous = compareVersions(state.version, version) < 0 ? state.version : null;
    return { ...state, version, previous, welcome: false };
  }
  // A legacy settings file without Crosshair Studio predates the 0.4 series.
  // An older 0.4 settings file has no exact version marker. The redesign is
  // the only feature known to be new for that cohort.
  return {
    version,
    previous: origin === 'pre-0.4' ? '0.3.1-alpha.9' : origin === 'existing' && version === '0.4.0' ? '0.4.0-rc.3' : null,
    welcome: origin === 'fresh' && !oldTourDone,
    outcomes: oldTourDone ? { welcome: 'completed' } : {},
  };
}

export function pendingTour(state: TourState, manifest: readonly Release[] = RELEASES): 'welcome' | 'update' | null {
  if (state.welcome && !state.outcomes.welcome) return 'welcome';
  if (state.previous && !state.outcomes['release:' + state.version] && manifest.some(r => r.version === state.version)) return 'update';
  return null;
}

export function featuresFor(release: Release, previous: string | null) {
  return release.features.filter(feature => !previous || compareVersions(previous, feature.since) < 0);
}
