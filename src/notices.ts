/** Home's status message: which one to show, given the engine's current
 * message and the occurrence the user closed. Kept free of React and Tauri
 * so it can be tested.
 *
 * The engine gives every occurrence an id (`Status.notice_id`): a new
 * message, or the same text coming back after it was closed or cleared, gets
 * a new id; the same message reported again while it is showing keeps its
 * id. Closing hides exactly that occurrence, so:
 * - repeats of a message that is still showing never re-open it;
 * - the same message later (a new occurrence) is shown again;
 * - closing one message never hides a different one. */
export type Notice = { text: string; id: number };

export function visibleNotice(text: string | null | undefined, id: number, closedId: number | null): Notice | null {
  if (!text) return null;
  return id === closedId ? null : { text, id };
}
