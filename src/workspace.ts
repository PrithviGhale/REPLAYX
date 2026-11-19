export const WORKSPACES = [
  { title: 'Capture', pages: [
    { id: 'home', label: 'Overview' }, { id: 'replay', label: 'Instant Replay' },
    { id: 'capture', label: 'Recording' }, { id: 'audio', label: 'Audio' },
  ] },
  { title: 'Library', pages: [{ id: 'clips', label: 'Clips' }, { id: 'games', label: 'Games' }] },
  { title: 'Studio', pages: [{ id: 'crosshair', label: 'Crosshair Studio' }] },
  { title: 'Settings', pages: [{ id: 'settings', label: 'Preferences' }, { id: 'performance', label: 'Performance' }] },
] as const;

export type Page = typeof WORKSPACES[number]['pages'][number]['id'];
export const SETTINGS_TABS = ['general', 'hotkeys', 'updates', 'about'] as const;
export type SettingsTab = typeof SETTINGS_TABS[number];
export type Route = { page: Page; tab: SettingsTab };

export const PAGES = WORKSPACES.flatMap(workspace => [...workspace.pages]);
export const DEFAULT_ROUTE: Route = { page: 'home', tab: 'general' };

export function parseRoute(hash: string): Route | null {
  const [id, tab, extra] = hash.replace(/^#/, '').split(':');
  const match = PAGES.find(page => page.id === id);
  if (!match || extra !== undefined || (tab !== undefined && id !== 'settings')) return null;
  return { page: match.id, tab: SETTINGS_TABS.find(option => option === tab) ?? 'general' };
}

export function routeHash({ page, tab }: Route) {
  return '#' + (page === 'settings' && tab !== 'general' ? page + ':' + tab : page);
}
