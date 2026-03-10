use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Mutex;

#[derive(Debug)]
pub enum IpcError {
    NotImplemented,
    InvalidEndpoint,
    Disconnected,
    BufferTooSmall,
}

pub type IpcResult<T> = Result<T, IpcError>;
pub const DAEMON_LOCAL_SOCKET_PATH: &str = "/tmp/zerolink_connectord.sock";

pub trait ControlChannel: Send + Sync {
    fn send(&self, _data: &[u8]) -> IpcResult<()> {
        Err(IpcError::NotImplemented)
    }

    fn recv(&self, _buf: &mut [u8]) -> IpcResult<usize> {
        Err(IpcError::NotImplemented)
    }
}

#[derive(Default)]
struct LoopbackControlChannel {
    queue: Mutex<VecDeque<Vec<u8>>>,
}

impl ControlChannel for LoopbackControlChannel {
    fn send(&self, data: &[u8]) -> IpcResult<()> {
        let mut queue = self.queue.lock().map_err(|_| IpcError::NotImplemented)?;
        queue.push_back(data.to_vec());
        Ok(())
    }

    fn recv(&self, buf: &mut [u8]) -> IpcResult<usize> {
        let mut queue = self.queue.lock().map_err(|_| IpcError::NotImplemented)?;
        let Some(msg) = queue.pop_front() else {
            return Err(IpcError::Disconnected);
        };
        if buf.len() < msg.len() {
            return Err(IpcError::BufferTooSmall);
        }
        let len = msg.len();
        buf[..len].copy_from_slice(&msg);
        Ok(len)
    }
}

#[cfg(unix)]
struct UnixSocketControlChannel {
    stream: Mutex<std::os::unix::net::UnixStream>,
}

#[cfg(unix)]
impl ControlChannel for UnixSocketControlChannel {
    fn send(&self, data: &[u8]) -> IpcResult<()> {
        let mut stream = self.stream.lock().map_err(|_| IpcError::NotImplemented)?;
        let len = u32::try_from(data.len()).map_err(|_| IpcError::BufferTooSmall)?;
        stream
            .write_all(&len.to_le_bytes())
            .map_err(|_| IpcError::Disconnected)?;
        stream.write_all(data).map_err(|_| IpcError::Disconnected)?;
        stream.flush().map_err(|_| IpcError::Disconnected)?;
        Ok(())
    }

    fn recv(&self, buf: &mut [u8]) -> IpcResult<usize> {
        let mut stream = self.stream.lock().map_err(|_| IpcError::NotImplemented)?;
        let mut len_bytes = [0u8; 4];
        stream
            .read_exact(&mut len_bytes)
            .map_err(|_| IpcError::Disconnected)?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        if buf.len() < len {
            return Err(IpcError::BufferTooSmall);
        }
        stream
            .read_exact(&mut buf[..len])
            .map_err(|_| IpcError::Disconnected)?;
        Ok(len)
    }
}

pub fn daemon_socket_path(endpoint: &str) -> IpcResult<&'static str> {
    match endpoint {
        "daemon://local" => Ok(DAEMON_LOCAL_SOCKET_PATH),
        v if v.starts_with("daemon://") => Err(IpcError::InvalidEndpoint),
        _ => Err(IpcError::InvalidEndpoint),
    }
}

pub fn connect_control_channel(endpoint: &str) -> IpcResult<Box<dyn ControlChannel>> {
    if endpoint == "inproc://loopback" {
        return Ok(Box::new(LoopbackControlChannel::default()));
    }
    if endpoint.starts_with("daemon://") {
        #[cfg(unix)]
        {
            let path = daemon_socket_path(endpoint)?;
            let stream = std::os::unix::net::UnixStream::connect(path)
                .map_err(|_| IpcError::Disconnected)?;
            return Ok(Box::new(UnixSocketControlChannel {
                stream: Mutex::new(stream),
            }));
        }
        #[cfg(not(unix))]
        {
            let _ = endpoint;
            return Err(IpcError::Disconnected);
        }
    }
    Err(IpcError::InvalidEndpoint)
}

pub fn control_request(
    endpoint: &str,
    request: &[u8],
    response_buf: &mut [u8],
) -> IpcResult<usize> {
    let channel = connect_control_channel(endpoint)?;
    channel.send(request)?;
    channel.recv(response_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_channel_roundtrip() {
        let ch = connect_control_channel("inproc://loopback").expect("connect should succeed");
        ch.send(b"ping").expect("send should succeed");
        let mut buf = [0u8; 8];
        let len = ch.recv(&mut buf).expect("recv should succeed");
        assert_eq!(&buf[..len], b"ping");
    }

    #[test]
    fn daemon_endpoint_reports_disconnected() {
        let res = connect_control_channel("daemon://local");
        assert!(matches!(res, Err(IpcError::Disconnected)));
    }
}
