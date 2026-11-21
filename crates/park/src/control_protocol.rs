//! Park's Wayland control protocol and sealed response-file transport.

use std::{
    fs::File,
    io::{self, ErrorKind::InvalidData, Write},
};

use lince_host::control::RESPONSE_LIMIT;
use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};

/// Server bindings for desktop control.
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
/// Client bindings for desktop control.
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

const RESPONSE_SEALS: SealFlags = SealFlags::SEAL
    .union(SealFlags::SHRINK)
    .union(SealFlags::GROW)
    .union(SealFlags::WRITE);

pub(crate) fn response_file(bytes: &[u8]) -> io::Result<File> {
    if bytes.len() > RESPONSE_LIMIT {
        return Err(io::Error::new(
            InvalidData,
            "The management response exceeds its size bound",
        ));
    }
    let mut file = File::from(memfd_create(
        c"park-control-response",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )?);
    file.write_all(bytes)?;
    fcntl_add_seals(&file, RESPONSE_SEALS)?;
    Ok(file)
}

#[cfg(any(feature = "server-tests", feature = "nested"))]
pub(crate) fn read_response(fd: std::os::fd::OwnedFd) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;

    use rustix::fs::{FileType, fcntl_get_seals, fstat};
    let file = File::from(fd);
    let stat = fstat(&file)?;
    let size = match usize::try_from(stat.st_size) {
        Ok(size)
            if size <= RESPONSE_LIMIT
                && FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile
                && fcntl_get_seals(&file)?.contains(RESPONSE_SEALS) =>
        {
            size
        }
        _ => {
            return Err(io::Error::new(
                InvalidData,
                "The management response must be a bounded sealed regular file",
            ));
        }
    };
    let mut bytes = vec![0; size];
    file.read_exact_at(&mut bytes, 0)?;
    Ok(bytes)
}

#[cfg(all(test, feature = "server-tests"))]
mod tests {
    use std::os::unix::fs::FileExt;

    use super::*;

    #[test]
    fn response_files_preserve_large_bounded_payloads_and_refuse_mutation_oversize_and_streams() {
        let bytes = vec![b'x'; RESPONSE_LIMIT];
        let file = response_file(&bytes).unwrap();
        assert!(file.write_all_at(b"y", 0).is_err());
        assert_eq!(
            read_response(file.try_clone().unwrap().into()).unwrap(),
            bytes
        );
        assert!(response_file(&vec![0; bytes.len() + 1]).is_err());
        let unsealed =
            File::from(memfd_create(c"unsealed-control-response", MemfdFlags::CLOEXEC).unwrap());
        unsealed.write_all_at(b"{}", 0).unwrap();
        assert!(read_response(unsealed.into()).is_err());
        let oversized = File::from(
            memfd_create(
                c"oversized-control-response",
                MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
            )
            .unwrap(),
        );
        oversized.set_len(bytes.len() as u64 + 1).unwrap();
        fcntl_add_seals(&oversized, RESPONSE_SEALS).unwrap();
        assert!(read_response(oversized.into()).is_err());
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(read_response(socket.into()).is_err());
    }
}
