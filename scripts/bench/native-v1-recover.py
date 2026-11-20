"""Recover completed native V1 segments by stream copy; never opens capture devices."""
import argparse
import json
import psutil
from pathlib import Path
from native_v1_support import ReplayStore
ROOT=Path(__file__).resolve().parents[2]
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--session',type=Path,required=True)
    parser.add_argument('--filename',default='recovered.mp4');parser.add_argument('--seconds',type=int,default=1800)
    args=parser.parse_args();root=args.session.resolve()
    if not root.is_relative_to((ROOT/'test-output').resolve()):parser.error('Only performance-worktree test sessions are supported')
    status=json.loads((root/'status.json').read_text())
    try:
        process=psutil.Process(status['controller_pid'])
        still_running=abs(process.create_time()-status['controller_created'])<.001
    except (psutil.NoSuchProcess,KeyError):still_running=False
    if still_running:parser.error('Stop the session before recovery')
    if status.get('state') not in ('complete','fallback_required','recording','starting'):parser.error('Unrecognized session state')
    entries=json.loads((root/'recovery.json').read_text())['segments']
    segments=[]
    for entry in entries:
        path=Path(entry['path']).resolve()
        if not path.is_relative_to(root) or path.suffix!='.mkv':raise ValueError('Invalid recovery segment path')
        if path.is_file():segments.append(dict(entry,path=path))
    if not segments:raise ValueError('No completed segments remain')
    end=segments[-1]['end'];segments=[s for s in segments if s['end']>end-args.seconds]
    lock=json.loads((ROOT/'tools/ffmpeg-lock.json').read_text(encoding='utf-8-sig'));ffmpeg=ROOT/'tools'/lock['runtime_dir']/'ffmpeg.exe'
    store=ReplayStore(root,ffmpeg,recording=False)
    try: print(store.save(args.seconds,args.filename,segments).result(timeout=30))
    finally:store.close()
if __name__=='__main__':main()
