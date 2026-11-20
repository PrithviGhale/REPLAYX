// Isolated developer launch. No installed profile, startup entry, installer or updater.
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
if(basename(root).toLowerCase()!=='replayx-perf') throw new Error('Run this isolated launcher from replayx-perf.');
const generated=process.argv.includes('--generated');
const phaseLock=process.argv.includes('--phase-lock');
const folder=join(root,'test-output',(generated?'native-desktop-generated':'native-desktop')+(phaseLock?'-phase-lock':''));
const data=join(folder,'data');mkdirSync(data,{recursive:true});
const settings=join(data,'settings.json');
if(!existsSync(settings)) writeFileSync(settings,JSON.stringify({recording_engine:'ffmpeg',output_dir:join(folder,'Clips'),
  tour_done:true,settings_version:6,launch_at_startup:false,updates:{auto_check:false,auto_download:false,auto_install:false},
  audio:[{kind:'system',name:'System audio',enabled:true},{kind:'microphone',name:'Microphone',enabled:false}]},null,2),{encoding:'utf8',flag:'wx'});
const env={...process.env,REPLAYX_NATIVE_ALPHA:'1',REPLAYX_NATIVE_EXE:join(root,'src-tauri/target/release/replayx-native-proto.exe'),REPLAYX_DATA_DIR:data};
delete env.REPLAYX_NATIVE_TIMESTAMP_TEST;delete env.REPLAYX_NATIVE_PHASE_SOURCE;delete env.REPLAYX_NATIVE_TRACE_ROI;delete env.REPLAYX_NATIVE_PHYSICAL_AUDIO;delete env.REPLAYX_NATIVE_TEST_SECONDS;delete env.REPLAYX_ALPHA_SYNTHETIC;
if(generated)env.REPLAYX_ALPHA_SYNTHETIC='1';
if(phaseLock)env.REPLAYX_NATIVE_TIMESTAMP_TEST='phase-lock';
const run=(exe,args)=>{const result=spawnSync(exe,args,{cwd:root,env,stdio:'inherit',windowsHide:true});if(result.error)throw result.error;if(result.status!==0)process.exit(result.status??1);};
run('cargo',['build','--manifest-path','src-tauri/Cargo.toml','--offline','--release','--bin','replayx-native-proto']);
console.log(`Native Alpha settings: ${data}`);
if(phaseLock)console.log('Explicit phase-lock experiment. This separate profile still defaults to FFmpeg; low-rate mixed-desktop cadence remains under validation.');
console.log(`Clips: ${join(folder,'Clips')}`);
console.log(generated?'GENERATED TEST: no desktop or audio devices are captured.':'FFmpeg is the default. Select Native under Video > Recording engine, then Save changes.');
run(process.execPath,[join(root,'node_modules/@tauri-apps/cli/tauri.js'),'dev','--config',join(root,'src-tauri/tauri.native.conf.json')]);
