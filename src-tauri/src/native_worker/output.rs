//! Optional bounded loopback transport for the experimental muxer.
use anyhow::{bail, Result};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
pub enum Target {
    File(File),
    Stream(TcpStream),
}
impl Target {
    pub fn open(path: &str, address: Option<&str>) -> Result<Self> {
        if let Some(address) = address {
            let address: SocketAddr = address.parse()?;
            if !address.ip().is_loopback() {
                bail!("Native stream must stay on loopback")
            }
            let socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
            socket.set_write_timeout(Some(Duration::from_secs(2)))?;
            socket.set_nodelay(true)?;
            Ok(Self::Stream(socket))
        } else {
            Ok(Self::File(
                OpenOptions::new().write(true).create_new(true).open(path)?,
            ))
        }
    }
    pub fn sync(&self) -> io::Result<()> {
        match self {
            Self::File(file) => file.sync_data(),
            Self::Stream(_) => Ok(()),
        }
    }
}
impl Write for Target {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.write(bytes),
            Self::Stream(socket) => socket.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::Stream(socket) => socket.flush(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_remote_destinations_and_existing_files() {
        assert!(Target::open("unused", Some("192.0.2.1:1234")).is_err());
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("keep");
        std::fs::write(&file, b"keep").unwrap();
        assert!(Target::open(file.to_str().unwrap(), None).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
