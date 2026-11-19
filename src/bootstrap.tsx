import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { AppProvider } from './context';
import './style.css';
import './workspace.css';
import { native } from './api';
import { splashDone, splashStep } from './splash';
import type { Release } from './release-tour';

export function mount(tourFixture?: Release) {
  const syncMotion = () => {
    document.documentElement.toggleAttribute('data-ui-paused', document.visibilityState !== 'visible' || !document.hasFocus());
  };
  document.addEventListener('visibilitychange', syncMotion);
  window.addEventListener('focus', syncMotion);
  window.addEventListener('blur', syncMotion);
  syncMotion();
  splashStep('Preparing interface');
  createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <AppProvider><App tourFixture={tourFixture} /></AppProvider>
    </React.StrictMode>,
  );
  if (!native) requestAnimationFrame(splashDone);
}
