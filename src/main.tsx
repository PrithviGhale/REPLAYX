import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { AppProvider } from './context';
import './style.css';
import { native } from './api';
import { splashDone, splashStep } from './splash';

splashStep('Preparing interface');

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppProvider><App /></AppProvider>
  </React.StrictMode>,
);
// The browser preview has no engine to wait for.
if (!native) requestAnimationFrame(() => splashDone());
