use std::collections::HashMap;
#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::fd::RawFd;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};

#[derive(Debug)]
pub enum ShmError {
    InvalidArg,
    ShmExhausted,
    NotFound,
    InvalidRange,
}

pub type ShmResult<T> = Result<T, ShmError>;

#[derive(Debug, Clone, Copy)]
pub struct ShmConfig {
    pub hard_limit_bytes: u64,
    pub sweep_interval_ms: u64,
}

impl Default for ShmConfig {
    fn default() -> Self {
        Self {
            hard_limit_bytes: 512 * 1024 * 1024,
            sweep_interval_ms: 100,
        }
    }
}

enum BufferSlot {
    #[cfg(unix)]
    UnixShm {
        ptr: *mut u8,
        len: usize,
        fd: RawFd,
        name: CString,
    },
    Heap(Box<[u8]>),
    #[cfg(windows)]
    WindowsMap {
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        len: usize,
        mapping: HANDLE,
    },
}

pub struct ShmManager {
    config: ShmConfig,
    next_id: u64,
    used_bytes: u64,
    slots: HashMap<u64, BufferSlot>,
}

impl ShmManager {
    pub fn new(config: ShmConfig) -> Self {
        Self {
            config,
            next_id: 1,
            used_bytes: 0,
            slots: HashMap::new(),
        }
    }

    #[cfg(unix)]
    fn alloc_unix_shm(&mut self, id: u64, size: u32) -> ShmResult<(u64, *mut u8)> {
        let name = format!("/zl_shm_{}_{}", std::process::id(), id);
        let cname = CString::new(name).map_err(|_| ShmError::InvalidArg)?;
        let fd = unsafe {
            libc::shm_open(
                cname.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600,
            )
        };
        if fd < 0 {
            return Err(ShmError::ShmExhausted);
        }

        let len = size as usize;
        if unsafe { libc::ftruncate(fd, len as libc::off_t) } != 0 {
            unsafe {
                libc::close(fd);
                libc::shm_unlink(cname.as_ptr());
            }
            return Err(ShmError::ShmExhausted);
        }

        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if map == libc::MAP_FAILED {
            unsafe {
                libc::close(fd);
                libc::shm_unlink(cname.as_ptr());
            }
            return Err(ShmError::ShmExhausted);
        }

        let ptr = map as *mut u8;
        self.slots.insert(
            id,
            BufferSlot::UnixShm {
                ptr,
                len,
                fd,
                name: cname,
            },
        );
        Ok((id, ptr))
    }

    #[cfg(windows)]
    fn alloc_windows_map(&mut self, id: u64, size: u32) -> ShmResult<(u64, *mut u8)> {
        let len = size as usize;
        let mapping = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                size,
                std::ptr::null(),
            )
        };
        if mapping.is_null() {
            return Err(ShmError::ShmExhausted);
        }

        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, len) };
        if view.Value.is_null() {
            unsafe {
                CloseHandle(mapping);
            }
            return Err(ShmError::ShmExhausted);
        }

        let ptr = view.Value as *mut u8;
        self.slots
            .insert(id, BufferSlot::WindowsMap { view, len, mapping });
        Ok((id, ptr))
    }

    pub fn alloc(&mut self, size: u32) -> ShmResult<(u64, *mut u8)> {
        if size == 0 {
            return Err(ShmError::InvalidArg);
        }
        let size_u64 = size as u64;
        if self.used_bytes.saturating_add(size_u64) > self.config.hard_limit_bytes {
            return Err(ShmError::ShmExhausted);
        }

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if id == 0 {
            return Err(ShmError::InvalidArg);
        }

        #[cfg(unix)]
        let unix_alloc = self.alloc_unix_shm(id, size);
        #[cfg(unix)]
        let (id, ptr) = match unix_alloc {
            Ok(v) => v,
            Err(_) => {
                let mut data = vec![0u8; size as usize].into_boxed_slice();
                let ptr = data.as_mut_ptr();
                self.slots.insert(id, BufferSlot::Heap(data));
                (id, ptr)
            }
        };
        #[cfg(not(unix))]
        let (id, ptr) = {
            #[cfg(windows)]
            {
                match self.alloc_windows_map(id, size) {
                    Ok(v) => v,
                    Err(_) => {
                        let mut data = vec![0u8; size as usize].into_boxed_slice();
                        let ptr = data.as_mut_ptr();
                        self.slots.insert(id, BufferSlot::Heap(data));
                        (id, ptr)
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let mut data = vec![0u8; size as usize].into_boxed_slice();
                let ptr = data.as_mut_ptr();
                self.slots.insert(id, BufferSlot::Heap(data));
                (id, ptr)
            }
        };

        self.used_bytes = self.used_bytes.saturating_add(size_u64);
        Ok((id, ptr))
    }

    pub fn release(&mut self, id: u64) -> ShmResult<()> {
        if id == 0 {
            return Err(ShmError::InvalidArg);
        }
        let Some(slot) = self.slots.remove(&id) else {
            return Err(ShmError::NotFound);
        };
        match slot {
            #[cfg(unix)]
            BufferSlot::UnixShm { ptr, len, fd, name } => {
                unsafe {
                    libc::munmap(ptr as *mut libc::c_void, len);
                    libc::close(fd);
                    libc::shm_unlink(name.as_ptr());
                }
                self.used_bytes = self.used_bytes.saturating_sub(len as u64);
            }
            BufferSlot::Heap(data) => {
                self.used_bytes = self.used_bytes.saturating_sub(data.len() as u64);
            }
            #[cfg(windows)]
            BufferSlot::WindowsMap { view, len, mapping } => {
                unsafe {
                    UnmapViewOfFile(view);
                    CloseHandle(mapping);
                }
                self.used_bytes = self.used_bytes.saturating_sub(len as u64);
            }
        }
        Ok(())
    }

    pub fn read_range(&self, id: u64, offset: u32, length: u32) -> ShmResult<Vec<u8>> {
        let Some(slot) = self.slots.get(&id) else {
            return Err(ShmError::NotFound);
        };
        let start = offset as usize;
        let len = length as usize;
        let end = start.saturating_add(len);
        match slot {
            #[cfg(unix)]
            BufferSlot::UnixShm { ptr, len, .. } => {
                if end > *len {
                    return Err(ShmError::InvalidRange);
                }
                let bytes = unsafe { std::slice::from_raw_parts(*ptr as *const u8, *len) };
                Ok(bytes[start..end].to_vec())
            }
            BufferSlot::Heap(data) => {
                if end > data.len() {
                    return Err(ShmError::InvalidRange);
                }
                Ok(data[start..end].to_vec())
            }
            #[cfg(windows)]
            BufferSlot::WindowsMap { view, len, .. } => {
                if end > *len {
                    return Err(ShmError::InvalidRange);
                }
                let bytes =
                    unsafe { std::slice::from_raw_parts(view.Value as *const u8, *len) };
                Ok(bytes[start..end].to_vec())
            }
        }
    }
}

impl Default for ShmManager {
    fn default() -> Self {
        Self::new(ShmConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_release_updates_usage() {
        let mut m = ShmManager::new(ShmConfig {
            hard_limit_bytes: 16,
            sweep_interval_ms: 100,
        });
        let (id, _ptr) = m.alloc(8).expect("alloc should succeed");
        assert!(m.release(id).is_ok());
    }

    #[test]
    fn alloc_respects_hard_limit() {
        let mut m = ShmManager::new(ShmConfig {
            hard_limit_bytes: 4,
            sweep_interval_ms: 100,
        });
        assert!(matches!(m.alloc(8), Err(ShmError::ShmExhausted)));
    }

    #[test]
    fn read_range_roundtrip() {
        let mut m = ShmManager::default();
        let (id, ptr) = m.alloc(5).expect("alloc");
        unsafe {
            std::ptr::copy_nonoverlapping(b"hello".as_ptr(), ptr, 5);
        }
        let got = m.read_range(id, 1, 3).expect("read");
        assert_eq!(got, b"ell");
        let _ = m.release(id);
    }

    #[test]
    fn read_range_out_of_bounds_fails() {
        let mut m = ShmManager::default();
        let (id, _ptr) = m.alloc(2).expect("alloc");
        let err = m.read_range(id, 1, 2).expect_err("should fail");
        assert!(matches!(err, ShmError::InvalidRange));
        let _ = m.release(id);
    }
}
