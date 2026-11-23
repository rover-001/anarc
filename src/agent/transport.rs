//! Local transport for Agent Control.
//!
//! Unix uses Unix domain sockets, unchanged. Windows uses a loopback-only TCP
//! listener whose port and bearer token are written to an endpoint file in the
//! per-user data directory. Both expose the same byte stream and keep the
//! newline-delimited JSON protocol byte-for-byte identical.

use std::io::{self, Read, Write};
#[cfg(windows)]
use std::net::TcpStream;
use std::net::{Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[derive(Clone, Debug)]
pub enum Endpoint {
    #[cfg(unix)]
    Unix(PathBuf),
    #[cfg(windows)]
    Tcp { addr: SocketAddr, token: String },
}

impl Endpoint {
    pub fn display(&self) -> String {
        match self {
            #[cfg(unix)]
            Self::Unix(path) => path.to_string_lossy().into_owned(),
            #[cfg(windows)]
            Self::Tcp { addr, .. } => format!("tcp://{addr}"),
        }
    }
}

pub enum Stream {
    #[cfg(unix)]
    Unix(UnixStream),
    #[cfg(windows)]
    Tcp(TcpStream),
}

impl Stream {
    pub fn connect(endpoint: &Endpoint) -> io::Result<Self> {
        match endpoint {
            #[cfg(unix)]
            Endpoint::Unix(path) => UnixStream::connect(path).map(Self::Unix),
            #[cfg(windows)]
            Endpoint::Tcp { addr, .. } => TcpStream::connect(addr).map(Self::Tcp),
        }
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.set_read_timeout(timeout),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.set_read_timeout(timeout),
        }
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.set_write_timeout(timeout),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.set_write_timeout(timeout),
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.try_clone().map(Self::Unix),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.try_clone().map(Self::Tcp),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.read(buf),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.write(buf),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.flush(),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.flush(),
        }
    }
}

/// 256-bit bearer token, used only on Windows to reject processes that can
/// reach the loopback port but cannot read the per-user endpoint file.
pub fn new_token() -> io::Result<String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("unable to generate agent token"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push_str(&format!("{byte:02x}"));
    }
    Ok(token)
}

pub fn loopback_addr(port: u16) -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)
}
