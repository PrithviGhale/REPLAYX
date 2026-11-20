"""Offline windowed-fixture motion audit. Never captures devices or plays audio."""
import argparse, collections, json, subprocess
from pathlib import Path

def motion_metrics(ticks, rendered):
    if len(ticks) < 2 or len(set(ticks)) < 10:
        raise ValueError("No sufficiently changing fixture counter was found")
    allowed = {row[1] for row in rendered}
    invalid = sum(t not in allowed for t in ticks)
    inc = [b-a for a,b in zip(ticks,ticks[1:])]
    hist = collections.Counter(inc)
    return {"frames":len(ticks),"unique_source_ticks":len(set(ticks)),"invalid_source_ticks":invalid,
        "repeated_pictures":hist[0],"repeated_picture_percent":100*hist[0]/len(inc),
        "skipped_fixture_ticks":sum(d-1 for d in inc if d>1),"backwards_steps":sum(d<0 for d in inc),
        "increments_histogram":dict(sorted(hist.items())),"first_tick":ticks[0],"last_tick":ticks[-1],
        "fixture_missed_ticks":sum(max(0,b[1]-a[1]-1) for a,b in zip(rendered,rendered[1:]))}

def main():
    ap=argparse.ArgumentParser(description=__doc__); ap.add_argument("clip",type=Path)
    ap.add_argument("--fixture-log",type=Path,required=True);ap.add_argument("--geometry",type=Path,required=True)
    a=ap.parse_args();root=Path(__file__).resolve().parents[2]
    lock=json.loads((root/"tools/ffmpeg-lock.json").read_text(encoding="utf-8-sig"));bins=root/"tools"/lock["runtime_dir"]
    g=json.loads(a.geometry.read_text()); source=json.loads(a.fixture_log.read_text())
    if not g.get("windowed") or g["width"]<100 or g["height"]<100:raise ValueError("A mapped windowed fixture is required")
    def run(cmd):return subprocess.run(list(map(str,cmd)),capture_output=True,check=True,timeout=90).stdout
    meta=json.loads(run([bins/"ffprobe.exe","-v","error","-select_streams","v:0","-show_frames","-show_entries","frame=best_effort_timestamp_time,key_frame,pict_type","-of","json",a.clip]))
    frames=meta["frames"]; ts=[float(f["best_effort_timestamp_time"]) for f in frames]
    raw=run([bins/"ffmpeg.exe","-v","error","-xerror","-threads","2","-filter_threads","1","-i",a.clip,"-map","0:v:0","-vf",f"crop={g['width']}:{g['height']}:{g['left']}:{g['top']},scale=320:180:flags=area,format=gray","-fps_mode","passthrough","-f","rawvideo","-"])
    ticks=[]
    for off in range(0,len(raw),320*180):
        pix=raw[off:off+320*180]
        if len(pix)!=320*180:raise ValueError("Truncated decoded frame")
        tick=sum((1<<i) for i in range(24) if sum(pix[(9+dy)*320+10+i*12+dx] for dy in range(4) for dx in range(4))/16>128)
        ticks.append(tick)
    m=motion_metrics(ticks,source); intervals=[(b-a)*1000 for a,b in zip(ts,ts[1:])]
    m.update({"clip":str(a.clip),"decoded_frames":len(frames),"keyframes":[i for i,f in enumerate(frames) if f["key_frame"]],"b_frames":sum(f.get("pict_type")=="B" for f in frames),"pts_monotonic":all(d>0 for d in intervals),"interval_ms_min":min(intervals),"interval_ms_max":max(intervals),"note":"Windowed desktop fixture; not gameplay FPS proof. Callback ticks are not a DWM scanout trace."})
    print(json.dumps(m,indent=2))
    if m["invalid_source_ticks"] or m["backwards_steps"]:raise SystemExit(1)
if __name__=="__main__":main()
