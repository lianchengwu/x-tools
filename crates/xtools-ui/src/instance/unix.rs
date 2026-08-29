use std::io::{self, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};

pub type InstanceListener = UnixListener;

pub fn claim_instance(name: &str) -> io::Result<Option<InstanceListener>> {
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    match UnixListener::bind_addr(&addr) {
        Ok(listener) => {
            listener.set_nonblocking(true)?;
            Ok(Some(listener))
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn terminate_instance(name: &str) -> io::Result<bool> {
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    match UnixStream::connect_addr(&addr) {
        Ok(mut stream) => {
            stream.write_all(b"QUIT\n")?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

pub fn raise_instance(name: &str, token: Option<&str>) -> io::Result<bool> {
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    match UnixStream::connect_addr(&addr) {
        Ok(mut stream) => {
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
            stream.write_all(line.as_bytes())?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

pub fn accept_command(listener: &InstanceListener) -> Option<super::InstanceCommand> {
    let (mut stream, _) = match listener.accept() {
        Ok(pair) => pair,
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => return None,
        Err(_) => return None,
    };
    let _ = stream.set_nonblocking(true);
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return None,
    };
    if n == 0 || n >= 4096 {
        return None;
    }
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    let line = text.lines().next()?.trim_end();
    if line.contains('\0') {
        return None;
    }
    if line == "QUIT" {
        return Some(super::InstanceCommand::Quit);
    }
    if line == "RAISE" {
        return Some(super::InstanceCommand::Raise(None));
    }
    let rest = line.strip_prefix("RAISE ")?;
    if rest.is_empty() || rest.contains(' ') {
        return None;
    }
    Some(super::InstanceCommand::Raise(Some(rest.to_string())))
}
