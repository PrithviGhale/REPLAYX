"""Measure whether a process's main window keeps answering Windows messages.

Every 100 ms sends WM_NULL with a 1 s timeout (what Windows itself does before
showing "Not responding") and prints one JSON line per sample:
  {"t": seconds since start, "latency_ms": ..., "hung": IsHungAppWindow, "foreground": bool}
Optionally brings the window to the foreground first, because REPLAYX's
0.2.1-alpha.1 freeze needed REPLAYX itself to be the foreground window.

Read-only apart from the foreground switch. Standard library only.
usage: python scripts/window-check.py --pid PID --seconds 30 [--foreground]
"""
import argparse
import ctypes
import ctypes.wintypes as w
import json
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)
WNDENUMPROC = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
user32.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]
user32.SendMessageTimeoutW.restype = ctypes.c_size_t
user32.GetForegroundWindow.restype = w.HWND
user32.IsHungAppWindow.argtypes = [w.HWND]


def main_window(pid):
    found = []

    def cb(hwnd, _):
        owner = w.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            n = user32.GetWindowTextLengthW(hwnd)
            if n > 0:
                found.append(hwnd)
        return True

    user32.EnumWindows(WNDENUMPROC(cb), 0)
    return found[0] if found else None


def bring_to_front(hwnd):
    # A process may only take the foreground right after input. A synthetic
    # F24 press (no app binds it) satisfies that rule; Alt would open the
    # target window's menu mode.
    VK_F24, KEYUP = 0x87, 0x0002
    user32.keybd_event(VK_F24, 0, 0, 0)
    user32.keybd_event(VK_F24, 0, KEYUP, 0)
    user32.ShowWindow(hwnd, 9)  # SW_RESTORE
    return bool(user32.SetForegroundWindow(hwnd))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", type=int, required=True)
    ap.add_argument("--seconds", type=float, default=30)
    ap.add_argument("--foreground", action="store_true")
    ap.add_argument("--keep-foreground", action="store_true", help="bring it back if another window takes the foreground")
    a = ap.parse_args()
    deadline = time.monotonic() + 30
    hwnd = None
    while hwnd is None and time.monotonic() < deadline:
        hwnd = main_window(a.pid)
        if hwnd is None:
            time.sleep(0.1)
    if hwnd is None:
        print(json.dumps({"error": "window not found"}), flush=True)
        return 1
    if a.foreground:
        print(json.dumps({"foreground_set": bring_to_front(hwnd)}), flush=True)
    start = time.monotonic()
    while time.monotonic() - start < a.seconds:
        t0 = time.perf_counter()
        result = ctypes.c_size_t()
        ok = user32.SendMessageTimeoutW(hwnd, 0, 0, 0, 0, 1000, ctypes.byref(result))  # WM_NULL, SMTO_NORMAL
        latency = (time.perf_counter() - t0) * 1000
        sample = {
            "t": round(time.monotonic() - start, 2),
            "latency_ms": round(latency, 1),
            "answered": bool(ok),
            "hung": bool(user32.IsHungAppWindow(hwnd)),
            "foreground": user32.GetForegroundWindow() == hwnd,
        }
        print(json.dumps(sample), flush=True)
        if a.keep_foreground and not sample["foreground"] and sample["answered"]:
            bring_to_front(hwnd)
        time.sleep(max(0, 0.1 - (time.perf_counter() - t0)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
