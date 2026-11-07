import {spawnSync} from 'node:child_process';
import {readFileSync,readdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.cwd();
const mediaLock=JSON.parse(readFileSync('tools/ffmpeg-lock.json','utf8').replace(/^\uFEFF/,''));
const media=path.join(root,'tools',mediaLock.runtime_dir);
function run(name,args,binary=false){const p=spawnSync(path.join(media,name+'.exe'),args,{encoding:binary?undefined:'utf8',maxBuffer:256*1024*1024,windowsHide:true});if(p.status!==0)throw Error(`${name} failed: ${p.stderr}`);return p.stdout;}
const arg=process.argv[2];
const reportPath=arg??readdirSync('test-output').filter(n=>n.startsWith('self-test-')).sort().reverse().map(n=>path.join('test-output',n,'report.json')).find(p=>{try{readFileSync(p);return true;}catch{return false;}});
assert(reportPath,'Run the native self-test first.');
const report=JSON.parse(readFileSync(reportPath,'utf8'));
const file=report.first;
const probe=JSON.parse(run('ffprobe',['-v','error','-show_streams','-show_format','-of','json',file]));
const audioStreams=probe.streams.filter(s=>s.codec_type==='audio');
function samples(index){const b=run('ffmpeg',['-v','error','-i',file,'-map',`0:a:${index}`,'-ac','1','-ar','48000','-f','f32le','-'],true);return Array.from({length:b.length/4},(_,i)=>b.readFloatLE(i*4));}
function power(samples,hz){let re=0,im=0;for(let i=0;i<samples.length;i++){const a=2*Math.PI*hz*i/48000;re+=samples[i]*Math.cos(a);im+=samples[i]*Math.sin(a);}return(re*re+im*im)/(samples.length*samples.length);}
const results={file,codec:probe.streams[0].codec_name,duration:Number(probe.format.duration),tracks:audioStreams.length,tests:[]};
const decoded=audioStreams.map((_,i)=>samples(i));
for(let i=0;i<decoded.length;i++){const x=decoded[i];const rms=Math.sqrt(x.reduce((s,v)=>s+v*v,0)/x.length);results.tests.push({test:`track ${i+1} decoded RMS`,value:rms});if(report.kind==='self-test')assert(rms>.001,'Unexpected silent synthetic audio');}
if(report.kind==='self-test'){
 assert.equal(audioStreams.length,3);
 const a=decoded[1],b=decoded[2];
 // Measure the central 50ms of a pulse, avoiding AAC boundary transients.
 const maxStart=(x)=>{let best=0,max=-1;for(let i=0;i<Math.min(x.length-2400,96000);i+=240){let sum=0;for(let n=i;n<i+2400;n++)sum+=x[n]*x[n];if(sum>max){max=sum;best=i;}}return best;};
 const sa=maxStart(a),sb=maxStart(b);const xa=a.slice(sa,sa+2400),xb=b.slice(sb,sb+2400);
 const isolationA=10*Math.log10(power(xa,440)/Math.max(power(xa,880),1e-20));
 const isolationB=10*Math.log10(power(xb,880)/Math.max(power(xb,440),1e-20));
 assert(isolationA>30&&isolationB>30,`Track leakage: ${isolationA}, ${isolationB} dB`);
 results.tests.push({test:'440Hz/880Hz independent track isolation',db:[isolationA,isolationB]});
 const frames=run('ffmpeg',['-v','error','-i',file,'-map','0:v:0','-vf','scale=1:1,format=gray','-fps_mode','passthrough','-f','rawvideo','-'],true);
 const frameRate=Number(probe.streams[0].avg_frame_rate.split('/')[0])/Number(probe.streams[0].avg_frame_rate.split('/')[1]);
 const flashes=[];for(let i=1;i<frames.length;i++){if(frames[i]>200&&frames[i-1]<40)flashes.push(i/frameRate);}
 const pulses=[];let prev=false;for(let i=0;i<a.length-480;i+=480){let rms=0;for(let n=i;n<i+480;n++)rms+=a[n]*a[n];const on=Math.sqrt(rms/480)>.025;if(on&&!prev)pulses.push(i/48000);prev=on;}
 const deltas=flashes.map(t=>Math.min(...pulses.map(p=>Math.abs(t-p))));
 assert(flashes.length>=3,'Synthetic video flash transitions not found');
 assert(Math.max(...deltas)<.12,`Synthetic A/V pulse alignment exceeds 120ms: ${deltas}`);
 results.tests.push({test:'Synthetic pulse/flash sync',events:deltas.length,max_offset_ms:Math.max(...deltas)*1000});
}
for(const stream of probe.streams){const packets=JSON.parse(run('ffprobe',['-v','error','-select_streams',String(stream.index),'-show_packets','-show_entries','packet=pts_time,dts_time,duration_time','-of','json',file])).packets;let previous=-Infinity,maxGap=0;for(const p of packets){const d=Number(p.dts_time);if(!Number.isFinite(d))continue;assert(d>=previous-.001,`Nonmonotonic stream ${stream.index}`);if(previous!==-Infinity)maxGap=Math.max(maxGap,d-previous);previous=d;}results.tests.push({test:`Stream ${stream.index} monotonic timestamps`,packets:packets.length,max_gap_ms:maxGap*1000});assert(maxGap<.1,'Timestamp gap exceeds 100ms');}
const output=path.join(path.dirname(reportPath),'media-verification.json');writeFileSync(output,JSON.stringify(results,null,2));console.log(JSON.stringify(results,null,2));
