use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug)]
pub enum IpcError {
    NotImplemented,
    InvalidEndpoint,
    Disconnected,
    BufferTooSmall,
}

pub type IpcResult<T> = Result<T, IpcError>;

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

pub fn connect_control_channel(endpoint: &str) -> IpcResult<Box<dyn ControlChannel>> {
    if endpoint == "inproc://loopback" {
        return Ok(Box::new(LoopbackControlChannel::default()));
    }
    if endpoint.starts_with("daemon://") {
        return Err(IpcError::Disconnected);
    }
    Err(IpcError::InvalidEndpoint)
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
