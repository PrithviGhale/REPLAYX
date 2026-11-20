"""Independent bounded spatial cadence reference; metadata only, no pixel reads."""
from collections import deque
from statistics import median
MAX_TRACKS=24
KEEP=32
MIN_EVENTS=9

def coverage(region,rects):
 x,y,w,h=region;mask=0
 for rx,ry,rw,rh in rects:
  bits=0
  for ix in range(8):
   cx=x+(2*ix+1)*w//16
   if rx<=cx<rx+rw:bits|=1<<ix
  if bits:
   for iy in range(8):
    cy=y+(2*iy+1)*h//16
    if ry<=cy<ry+rh:mask|=bits<<(iy*8)
 return mask.bit_count()>=60

class Tracker:
 def __init__(self,fps=60):
  self.period=10000000/fps;self.tracks=[];self.selected=None;self.switches=0
 def observe(self,time,rects,area):
  for r in rects:
   if r[2]<=0 or r[3]<=0 or r[2]*r[3]*20<area:continue
   if any(all(abs(a-b)<=8 for a,b in zip(t['rect'],r)) for t in self.tracks):continue
   if len(self.tracks)>=MAX_TRACKS:
    victim=min((t for t in self.tracks if t is not self.selected),key=lambda t:t['times'][-1] if t['times'] else -1)
    self.tracks.remove(victim)
   self.tracks.append({'rect':tuple(r),'times':deque(maxlen=KEEP),'updated':False})
  for t in self.tracks:
   t['updated']=coverage(t['rect'],rects)
   if t['updated'] and (not t['times'] or t['times'][-1]!=time):t['times'].append(time)
  def eligible(t):
   times=t['times']
   if len(times)<MIN_EVENTS or time-times[-1]>5000000:return False
   intervals=[b-a for a,b in zip(times,list(times)[1:]) if b>a]
   if len(intervals)<8:return False
   m=median(intervals)/self.period;n=1 if .7<=m<=1.3 else 2 if 1.7<=m<=2.3 else None
   return n is not None and sum(abs(x-n*self.period)<=.5*self.period+2 for x in intervals)*4>=len(intervals)*3
  def fast(t):
   times=t['times'];intervals=[b-a for a,b in zip(times,list(times)[1:]) if b>a]
   return len(intervals)>=8 and median(intervals)<.7*self.period and sum(x<.7*self.period for x in intervals)*4>=len(intervals)*3
  def pure(t):
   x,y,w,h=t['rect']
   for other in self.tracks:
    if other is t or not fast(other):continue
    rx,ry,rw,rh=other['rect'];intersection=max(0,min(x+w,rx+rw)-max(x,rx))*max(0,min(y+h,ry+rh)-max(y,ry))
    if intersection*3>=w*h:return False
   return True
  old=self.selected
  candidates=[t for t in self.tracks if eligible(t) and pure(t)]
  best=max(candidates,key=lambda t:t['rect'][2]*t['rect'][3]) if candidates else None
  if self.selected is None or not eligible(self.selected) or not pure(self.selected):self.selected=best
  elif best and best['rect'][2]*best['rect'][3]*4>=self.selected['rect'][2]*self.selected['rect'][3]*5:self.selected=best
  seed=[]
  if self.selected is not old:
   self.switches+=1
   if self.selected:seed=[t for t in self.selected['times'] if t<time]
  return {'changed':bool(self.selected and self.selected['updated']),'rect':self.selected['rect'] if self.selected else None,'seed':seed}
