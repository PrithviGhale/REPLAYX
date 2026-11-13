/** Retry helpers, kept free of React and Tauri so they can be tested. */

/** Repeat operations in order and stop at the first one that fails, so a
 * failed Instant Replay start never goes on to start a recording. Resolves
 * to whether all of them succeeded. */
export async function retryInOrder<T>(ops: readonly T[], attempt: (op: T) => Promise<boolean>): Promise<boolean> {
  if (ops.length === 0) return false;
  for (const op of ops) if (!(await attempt(op))) return false;
  return true;
}

/** Lets one piece of work run at a time: a call while it runs (a double
 * click) is ignored and resolves to `undefined`. */
export function createGuard() {
  let running = false;
  return {
    get running() { return running; },
    async run<R>(work: () => Promise<R>): Promise<R | undefined> {
      if (running) return undefined;
      running = true;
      try { return await work(); } finally { running = false; }
    },
  };
}
