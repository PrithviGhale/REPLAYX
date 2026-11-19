"""Independent, metadata-only timing policies and exact phase partitions."""
import bisect
HNS=10_000_000

def cfr(times,ids,origin,count,fps=60):
    return [ids[bisect.bisect_right(times,origin+n*HNS//fps)-1] for n in range(count)]

def phase_boundaries(times,origin,count,fps=60):
    """Test every selection regime, its equality boundaries, and both neighbors.
    For a fixed slot, selection changes only at capture_time-slot_offset.
    Thus this covers ALL integer 100 ns phases, not just a coarse sweep.
    """
    limit=(HNS+fps-1)//fps
    edges={0,limit-1}
    for t in times:
        n=(t-origin)*fps//HNS
        for slot in (n-1,n,n+1):
            if 0<=slot<count:
                d=t-origin-slot*HNS//fps
                for p in (d-1,d,d+1):
                    if 0<=p<limit:edges.add(p)
    ordered=sorted(edges)
    return sorted(edges|{(a+b)//2 for a,b in zip(ordered,ordered[1:])})

def source_spans(times):
    """Latest capture per representable 90 kHz tick. No content-ID input."""
    groups={}
    for i,t in enumerate(times):groups[((t-times[0])*90000+5_000_000)//HNS]=i
    return list(groups.values())

def transitions(ids):
    return {'repeats':sum(b==a for a,b in zip(ids,ids[1:])),
            'skipped_ids':sum(max(0,b-a-1) for a,b in zip(ids,ids[1:])),
            'backwards':sum(b<a for a,b in zip(ids,ids[1:]))}
