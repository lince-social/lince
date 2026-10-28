pub mod server {
    use smithay::reexports::wayland_server;
    use self::__interfaces::*;

    pub mod __interfaces {
        use smithay::reexports::wayland_server::backend as wayland_backend;
        wayland_scanner::generate_interfaces!("protocols/lince-park-control-v1.xml");
    }

    wayland_scanner::generate_server_code!("protocols/lince-park-control-v1.xml");
}

#[cfg(any(feature = "server-tests", feature = "nested"))]
pub mod client {
    use wayland_client;
    use self::__interfaces::*;

    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        wayland_scanner::generate_interfaces!("protocols/lince-park-control-v1.xml");
    }

    wayland_scanner::generate_client_code!("protocols/lince-park-control-v1.xml");
}
const _: &str = include_str!("../protocols/lince-park-control-v1.xml");

pub(crate) fn response_file(bytes: &[u8]) -> std::io::Result<std::fs::File> {
    use std::io::Write;
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    if bytes.len() > lince_interface::host::control::RESPONSE_LIMIT {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "The management response exceeds its size bound"));
    }
    let mut file = std::fs::File::from(memfd_create(c"park-control-response", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING)?);
    file.write_all(bytes)?;
    fcntl_add_seals(&file, SealFlags::SEAL | SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE)?;
    Ok(file)
}

#[cfg(any(feature = "server-tests", feature = "nested"))]
pub(crate) fn read_response(fd: std::os::fd::OwnedFd) -> std::io::Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;
    use rustix::fs::{FileType, SealFlags, fcntl_get_seals, fstat};
    let file = std::fs::File::from(fd);
    let stat = fstat(&file)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_size < 0
        || stat.st_size as u64 > lince_interface::host::control::RESPONSE_LIMIT as u64
        || !fcntl_get_seals(&file)?.contains(SealFlags::SEAL | SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE)
    { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "The management response must be a bounded sealed regular file")); }
    let mut bytes = vec![0; stat.st_size as usize];
    file.read_exact_at(&mut bytes, 0)?;
    Ok(bytes)
}

#[cfg(all(test, feature = "server-tests"))]
mod tests {
    use super::*;
    use std::os::unix::fs::FileExt;

    #[test]
    fn response_files_preserve_large_bounded_payloads_and_refuse_mutation_oversize_and_streams() {
        let bytes = vec![b'x'; lince_interface::host::control::RESPONSE_LIMIT];
        let file = response_file(&bytes).unwrap();
        assert!(file.write_all_at(b"y", 0).is_err());
        assert_eq!(read_response(file.try_clone().unwrap().into()).unwrap(), bytes);
        assert!(response_file(&vec![0; bytes.len() + 1]).is_err());
        let unsealed = std::fs::File::from(rustix::fs::memfd_create(c"unsealed-control-response", rustix::fs::MemfdFlags::CLOEXEC).unwrap());
        unsealed.write_all_at(b"{}", 0).unwrap();
        assert!(read_response(unsealed.into()).is_err());
        let oversized = std::fs::File::from(rustix::fs::memfd_create(c"oversized-control-response",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING).unwrap());
        oversized.set_len(bytes.len() as u64 + 1).unwrap();
        rustix::fs::fcntl_add_seals(&oversized, rustix::fs::SealFlags::SEAL | rustix::fs::SealFlags::SHRINK
            | rustix::fs::SealFlags::GROW | rustix::fs::SealFlags::WRITE).unwrap();
        assert!(read_response(oversized.into()).is_err());
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(read_response(socket.into()).is_err());
    }
}

