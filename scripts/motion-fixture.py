"""Silent fullscreen desktop motion target. Escape closes; never opens audio.
60Hz clock encoded in 24 blocks; scaled coordinates match frame-pacing.py.
Source callback times are logged so source stalls are distinguishable.
"""
import argparse
import ctypes
import json
import pathlib
import time
import tkinter as tk

parser = argparse.ArgumentParser()
parser.add_argument('--seconds', type=int, default=1200)
parser.add_argument('--log', type=pathlib.Path, required=True)
args = parser.parse_args()
# Tk's default Windows timer can wake only every 15.6ms, producing a poor
# 60Hz stimulus. Request precision for this test process, never busy-wait.
ctypes.windll.winmm.timeBeginPeriod(1)
ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
root = tk.Tk()
root.title('REPLAYX silent motion fixture — Escape to close')
root.attributes('-fullscreen', True)
root.attributes('-topmost', True)
canvas = tk.Canvas(root, highlightthickness=0, bg='#101820')
canvas.pack(fill='both', expand=True)
w, h = root.winfo_screenwidth(), root.winfo_screenheight()
sx, sy = w/320, h/180
bits = [canvas.create_rectangle((8+i*12)*sx, 7*sy, (16+i*12)*sx, 15*sy, outline='') for i in range(24)]
bars = [canvas.create_rectangle(0, (25+i*13)*sy, 25*sx, (34+i*13)*sy, fill=['#f0a020','#30c0c0','#c080f0'][i%3], outline='') for i in range(8)]
label = canvas.create_text(w/2, h*.86, fill='white', font=('Segoe UI', 26), text='')
start, last, samples = time.perf_counter(), -1, []
def close():
    args.log.parent.mkdir(parents=True, exist_ok=True)
    args.log.write_text(json.dumps(samples), encoding='utf-8')
    ctypes.windll.winmm.timeEndPeriod(1)
    root.destroy()
def draw():
    global last
    t = time.perf_counter()-start
    tick = int(t*60)
    if t >= args.seconds or args.log.with_suffix('.stop').exists():
        close()
        return
    if tick != last:
        samples.append([t, tick])
        for i, bit in enumerate(bits):
            canvas.itemconfigure(bit, fill='white' if tick & (1 << i) else 'black')
        for i, bar in enumerate(bars):
            x = (t*(60+i*15))%345-25
            canvas.coords(bar, x*sx, (25+i*13)*sy, (x+25)*sx, (34+i*13)*sy)
        canvas.itemconfigure(label, text=f'Silent motion test   {t:07.2f}s   tick {tick}\nEscape closes this test')
        last = tick
    root.after(1, draw)
root.bind('<Escape>', lambda _: close())
root.protocol('WM_DELETE_WINDOW', close)
draw()
root.mainloop()
