use std::{ffi::c_void, io, ptr::NonNull};

#[repr(C)]
struct StaticMetadata {
    maximum: f32,
    _average: f32,
    _minimum: f32,
    type1: bool,
    _sdr: bool,
    _hdr: bool,
    pq: bool,
    _hlg: bool,
}

#[repr(C)]
struct SignalColorimetry {
    _cycc: bool,
    _ycc: bool,
    rgb: bool,
    _st2113: bool,
    _ictcp: bool,
}

unsafe extern "C" {
    fn di_info_parse_edid(data: *const c_void, size: usize) -> *mut c_void;
    fn di_info_destroy(info: *mut c_void);
    fn di_info_get_hdr_static_metadata(info: *const c_void) -> *const StaticMetadata;
    fn di_info_get_supported_signal_colorimetry(info: *const c_void) -> *const SignalColorimetry;
}

struct Information(NonNull<c_void>);

impl Drop for Information {
    fn drop(&mut self) { unsafe { di_info_destroy(self.0.as_ptr()) }; }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Hdr {
    pub peak_nits: u32,
    pub reported: bool,
}

pub(super) fn hdr(bytes: &[u8]) -> io::Result<Hdr> {
    if bytes.len() < 128 || bytes.len() > 32768 || bytes.len() % 128 != 0 {
        return Err(io::Error::other("The display information has an invalid length."));
    }
    let information = Information(NonNull::new(unsafe {
        di_info_parse_edid(bytes.as_ptr().cast(), bytes.len())
    }).ok_or_else(|| io::Error::other("The display information could not be parsed."))?);
    let metadata = unsafe { di_info_get_hdr_static_metadata(information.0.as_ptr()).as_ref() }
        .ok_or_else(|| io::Error::other("The display has no HDR metadata information."))?;
    let colorimetry = unsafe { di_info_get_supported_signal_colorimetry(information.0.as_ptr()).as_ref() }
        .ok_or_else(|| io::Error::other("The display has no signal colorimetry information."))?;
    if !metadata.pq || !metadata.type1 || !colorimetry.rgb {
        return Err(io::Error::other("The display does not report HDR10 PQ, static metadata and BT.2020 RGB support."));
    }
    if metadata.maximum.is_finite() && metadata.maximum > 0.0 && metadata.maximum < 203.0 {
        return Err(io::Error::other("The display's reported peak is below Park's HDR reference white."));
    }
    let reported = metadata.maximum.is_finite() && metadata.maximum >= 203.0;
    Ok(Hdr {
        peak_nits: if reported { metadata.maximum.round().clamp(203.0, 1000.0) as u32 } else { 1000 },
        reported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_invalid_display_data_never_establishes_hdr_support() {
        for bytes in [vec![], vec![0; 127], vec![0; 128], vec![0; 129], vec![0; 32896]] {
            assert!(hdr(&bytes).is_err());
        }
    }
}
