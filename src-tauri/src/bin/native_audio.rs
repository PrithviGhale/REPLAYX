//! Explicit opt-in WASAPI bridge for native V1. Never reads application settings.
use anyhow::{bail, Context, Result};
use replayx_core::{audio, media, model::AudioSource, process_job::CaptureJob};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
fn arg(key: &str) -> Result<String> {
    let args: Vec<_> = std::env::args().collect();
    let index = args
        .iter()
        .position(|a| a == key)
        .with_context(|| format!("Missing {key}"))?;
    args.get(index + 1)
        .cloned()
        .context("Missing argument value")
}
fn publish(path: &PathBuf, value: &serde_json::Value) -> Result<()> {
    // Health is replaced atomically; readers may retry a Windows sharing violation.
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(value)?)?;
    for attempt in 0..20 {
        match fs::rename(&temporary, path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < 19 => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}
fn worker() -> Result<()> {
    let sources: Vec<AudioSource> = serde_json::from_slice(&fs::read(arg("--sources")?)?)?;
    if sources.is_empty()
        || sources.len() > 2
        || sources
            .iter()
            .any(|s| !s.enabled || !["system", "microphone"].contains(&s.kind.as_str()))
    {
        bail!("Explicit one/two enabled system or microphone sources required")
    }
    let stop_path = PathBuf::from(arg("--stop-file")?);
    let ready = PathBuf::from(arg("--ready")?);
    let health = PathBuf::from(arg("--health")?);
    let stop = Arc::new(AtomicBool::new(false));
    let failure = Arc::new(Mutex::new(None::<String>));
    let mut feeds = Vec::new();
    let mut readers = Vec::new();
    let mut threads = Vec::new();
    let mut endpoints = Vec::new();
    for source in sources {
        let feed = audio::spawn(source.clone())?;
        let socket = TcpStream::connect(
            feed.url
                .strip_prefix("tcp://")
                .context("Invalid audio URL")?,
        )?;
        socket.set_read_timeout(Some(Duration::from_millis(250)))?;
        let until = Instant::now() + Duration::from_secs(2);
        while feed.meter.timeline_origin_hns.load(Ordering::Acquire) == 0 {
            if Instant::now() >= until {
                bail!("Audio clock did not start")
            }
            thread::sleep(Duration::from_millis(5));
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        endpoints.push(json!({"url":format!("tcp://{}",listener.local_addr()?),"name":source.name,"kind":source.kind,
            "origin_qpc_hns":feed.meter.timeline_origin_hns.load(Ordering::Acquire)}));
        readers.push(socket.try_clone()?);
        let st = stop.clone();
        let error = failure.clone();
        threads.push(thread::spawn(move || {
            let result = (|| -> Result<()> {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut output = loop {
                    if st.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e.into()),
                    }
                    if Instant::now() >= deadline {
                        bail!("Native muxer did not connect to audio")
                    }
                    thread::sleep(Duration::from_millis(10));
                };
                output.set_write_timeout(Some(Duration::from_secs(1)))?;
                output.set_nodelay(true)?;
                let mut input = socket;
                let mut buffer = [0u8; 16384];
                while !st.load(Ordering::Relaxed) {
                    match input.read(&mut buffer) {
                        Ok(0) => bail!("WASAPI transport closed"),
                        Ok(n) => output.write_all(&buffer[..n])?,
                        Err(e)
                            if [std::io::ErrorKind::TimedOut, std::io::ErrorKind::WouldBlock]
                                .contains(&e.kind()) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                if !st.load(Ordering::Relaxed) {
                    *error.lock().unwrap() = Some(format!("{e:#}"));
                }
            }
        }));
        feeds.push(feed);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ready)?;
    serde_json::to_writer(&mut file, &json!({"feeds":endpoints}))?;
    file.flush()?;
    let until = Instant::now() + Duration::from_secs(arg("--seconds")?.parse()?);
    let mut terminal_error = None;
    while Instant::now() < until && !stop_path.exists() {
        let statuses:Vec<_>=feeds.iter().map(|f| json!({"bytes":f.meter.written_bytes.load(Ordering::Relaxed),
            "gaps":f.meter.gaps.load(Ordering::Relaxed),"discontinuities":f.meter.discontinuities.load(Ordering::Relaxed),
            "error":f.meter.error.lock().unwrap().clone()})).collect();
        terminal_error = failure.lock().unwrap().clone().or_else(|| {
            feeds
                .iter()
                .find_map(|f| f.meter.error.lock().unwrap().clone())
        });
        publish(
            &health,
            &json!({"healthy":terminal_error.is_none(),"feeds":statuses,"error":terminal_error}),
        )?;
        if terminal_error.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    stop.store(true, Ordering::Relaxed);
    for socket in readers {
        let _ = socket.shutdown(Shutdown::Both);
    }
    for handle in threads {
        let _ = handle.join();
    }
    drop(feeds);
    if let Some(e) = terminal_error {
        bail!(e)
    }
    Ok(())
}
fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--worker") {
        return worker();
    }
    let seconds: u64 = arg("--seconds")?.parse()?;
    if seconds > 7200 {
        bail!("Audio experiment limited to two hours")
    }
    let stop = PathBuf::from(arg("--stop-file")?);
    let mut child = media::command(std::env::current_exe()?)
        .args(std::env::args().skip(1))
        .arg("--worker")
        .spawn()?;
    let job = match CaptureJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            return Err(error);
        }
    };
    let mut until = Instant::now() + Duration::from_secs(seconds + 10);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("Audio worker failed: {status}")
            }
            return Ok(());
        }
        if stop.exists() {
            until = until.min(Instant::now() + Duration::from_secs(5));
        }
        if Instant::now() >= until {
            let _ = child.kill();
            drop(job);
            bail!("Audio worker shutdown timed out")
        }
        thread::sleep(Duration::from_millis(20));
    }
}
