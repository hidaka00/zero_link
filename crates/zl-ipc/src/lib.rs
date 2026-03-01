#[derive(Debug)]
pub enum IpcError {
    NotImplemented,
}

pub type IpcResult<T> = Result<T, IpcError>;

pub trait ControlChannel {
    fn send(&self, _data: &[u8]) -> IpcResult<()> {
        Err(IpcError::NotImplemented)
    }

    fn recv(&self, _buf: &mut [u8]) -> IpcResult<usize> {
        Err(IpcError::NotImplemented)
    }
}
