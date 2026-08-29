use std::io::{self, Error, ErrorKind};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BROKEN_PIPE,
    ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_NOT_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE, SetLastError,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    ReadFile, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PeekNamedPipe,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

struct Inner {
    handle: HANDLE,
    mutex: HANDLE,
    connected: bool,
    pending: Vec<u8>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
            CloseHandle(self.mutex);
        }
    }
}

#[derive(Clone)]
pub struct InstanceListener(Arc<Mutex<Inner>>);
impl InstanceListener {
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
}

fn current_user() -> io::Result<String> {
    let mut user = [0u16; 257];
    let mut len = user.len() as u32;
    if unsafe { GetUserNameW(user.as_mut_ptr(), &mut len) } == 0 {
        return Err(Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(
        &user[..len.saturating_sub(1) as usize],
    ))
}

fn endpoint_name(name: &str) -> io::Result<String> {
    Ok(format!("{}-{name}", current_user()?))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn pipe_name(endpoint: &str) -> Vec<u16> {
    wide(&format!(r"\\.\pipe\xtools-{endpoint}"))
}

fn create_pipe(name: &[u16]) -> io::Result<HANDLE> {
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(Error::last_os_error())
    } else {
        Ok(handle)
    }
}

pub fn claim_instance(name: &str) -> io::Result<Option<InstanceListener>> {
    let endpoint = endpoint_name(name)?;
    let pipe_name = pipe_name(&endpoint);
    let mutex_name = wide(&format!(r"Local\xtools-{endpoint}-instance"));
    unsafe { SetLastError(0) };
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if mutex.is_null() {
        return Err(Error::last_os_error());
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(mutex) };
        return Ok(None);
    }
    // We successfully claimed the unique instance mutex: we are the primary instance.
    // If a previous instance just exited or was killed, NPFS may take a brief moment
    // to finish releasing the dead pipe name. Retry while holding the mutex.
    let mut last_err = None;
    for _ in 0..50 {
        match create_pipe(&pipe_name) {
            Ok(handle) => {
                return Ok(Some(InstanceListener(Arc::new(Mutex::new(Inner {
                    handle,
                    mutex,
                    connected: false,
                    pending: Vec::new(),
                })))));
            }
            Err(err) => {
                last_err = Some(err);
                if let Ok(Some(handle)) = connect(&pipe_name) {
                    unsafe { CloseHandle(handle) };
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
        }
    }
    unsafe { CloseHandle(mutex) };
    Err(last_err.unwrap_or_else(|| {
        Error::new(
            ErrorKind::Other,
            "failed to create named pipe after acquiring instance mutex",
        )
    }))
}

fn connect(name: &[u16]) -> io::Result<Option<HANDLE>> {
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let code = unsafe { GetLastError() };
        if code == ERROR_PIPE_BUSY
            || code == ERROR_ACCESS_DENIED
            || code == ERROR_FILE_NOT_FOUND
            || code == ERROR_BROKEN_PIPE
        {
            Ok(None)
        } else {
            Err(Error::from_raw_os_error(code as i32))
        }
    } else {
        Ok(Some(handle))
    }
}

fn send(name: &str, bytes: &[u8]) -> io::Result<bool> {
    let name = pipe_name(&endpoint_name(name)?);
    let Some(handle) = connect(&name)? else {
        return Ok(false);
    };
    let mut offset = 0;
    while offset < bytes.len() {
        let mut written = 0;
        let result = unsafe {
            WriteFile(
                handle,
                bytes[offset..].as_ptr(),
                (bytes.len() - offset) as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            let err = Error::last_os_error();
            unsafe { CloseHandle(handle) };
            return Err(err);
        }
        if written == 0 {
            unsafe { CloseHandle(handle) };
            return Err(Error::new(
                ErrorKind::WriteZero,
                "named-pipe write made no progress",
            ));
        }
        offset += written as usize;
    }
    unsafe { CloseHandle(handle) };
    Ok(true)
}

pub fn terminate_instance(name: &str) -> io::Result<bool> {
    send(name, b"QUIT\n")
}

pub fn raise_instance(name: &str, token: Option<&str>) -> io::Result<bool> {
    let line = match token {
        Some(t)
            if !t.is_empty()
                && !t.contains('\0')
                && !t.contains(' ')
                && !t.contains('\n')
                && t.len() < 4000 =>
        {
            format!("RAISE {t}\n")
        }
        _ => "RAISE\n".to_string(),
    };
    send(name, line.as_bytes())
}

pub fn accept_command(listener: &InstanceListener) -> Option<super::InstanceCommand> {
    const MAX_COMMAND_SIZE: usize = 4096;
    let mut inner = listener.0.lock().ok()?;

    // Try connecting to a client if not marked connected
    if !inner.connected {
        if unsafe { ConnectNamedPipe(inner.handle, std::ptr::null_mut()) } == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_PIPE_CONNECTED || code == ERROR_NO_DATA {
                inner.connected = true;
            }
        } else {
            inner.connected = true;
        }
    }

    let mut available = 0;
    let peek_ok = unsafe {
        PeekNamedPipe(
            inner.handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };

    if peek_ok != 0 && available > 0 {
        inner.connected = true;
        let remaining = MAX_COMMAND_SIZE.saturating_sub(inner.pending.len());
        let mut buf = [0u8; MAX_COMMAND_SIZE];
        let mut read = 0;
        let read_ok = unsafe {
            ReadFile(
                inner.handle,
                buf.as_mut_ptr(),
                remaining.min(available as usize) as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if read_ok != 0 && read > 0 {
            inner.pending.extend_from_slice(&buf[..read as usize]);
        }
    } else if peek_ok == 0 {
        let code = unsafe { GetLastError() };
        if inner.connected
            && (code == ERROR_BROKEN_PIPE
                || code == ERROR_PIPE_NOT_CONNECTED
                || code == ERROR_NO_DATA)
        {
            if inner.pending.is_empty() {
                reset_connection(&mut inner);
                return None;
            }
        }
    }

    if let Some(end) = inner.pending.iter().position(|&byte| byte == b'\n') {
        let line = match std::str::from_utf8(&inner.pending[..end]) {
            Ok(line) => line.trim_end().to_string(),
            Err(_) => {
                reset_connection(&mut inner);
                return None;
            }
        };
        let command = if line == "QUIT" {
            super::InstanceCommand::Quit
        } else if line == "RAISE" {
            super::InstanceCommand::Raise(None)
        } else if let Some(rest) = line.strip_prefix("RAISE ") {
            if rest.is_empty() || rest.contains(' ') || rest.contains('\0') {
                reset_connection(&mut inner);
                return None;
            }
            super::InstanceCommand::Raise(Some(rest.to_string()))
        } else {
            reset_connection(&mut inner);
            return None;
        };
        reset_connection(&mut inner);
        return Some(command);
    }

    if inner.pending.len() >= MAX_COMMAND_SIZE {
        reset_connection(&mut inner);
        return None;
    }

    None
}
fn reset_connection(inner: &mut Inner) {
    unsafe { DisconnectNamedPipe(inner.handle) };
    inner.connected = false;
    inner.pending.clear();
}
