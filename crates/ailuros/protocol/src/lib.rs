use bytes::{Buf, BufMut};
pub use bytes::{Bytes, BytesMut};
use std::io::{self, ErrorKind};
use tokio_util::codec::{Decoder, Encoder};

pub const ALPN: &[u8] = b"lince/ailuros/1";
pub const HEADER_BYTES: usize = 5;
pub const MAX_AUTH_CONTROL: usize = 2048;
pub const MAX_CONTROL: usize = 4096;
pub const MAX_DATA: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Control = 0,
    Metadata = 1,
    Data = 2,
}

impl TryFrom<u8> for Kind {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Control),
            1 => Ok(Self::Metadata),
            2 => Ok(Self::Data),
            _ => Err(invalid("Unknown Ailuros frame kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Phase {
    #[default]
    Authentication,
    Streaming,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub kind: Kind,
    pub payload: Bytes,
}

impl Frame {
    pub fn new(kind: Kind, payload: impl Into<Bytes>) -> Self {
        Self {
            kind,
            payload: payload.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Codec {
    phase: Phase,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

impl Codec {
    pub fn new(phase: Phase) -> Self {
        Self { phase }
    }

    pub fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }

    fn validate(&self, kind: Kind, length: usize) -> io::Result<()> {
        let limit = match (self.phase, kind) {
            (Phase::Authentication, Kind::Control) => MAX_AUTH_CONTROL,
            (Phase::Authentication, _) => {
                return Err(invalid("Authentication required before stream data"));
            }
            (Phase::Streaming, Kind::Control | Kind::Metadata) => MAX_CONTROL,
            (Phase::Streaming, Kind::Data) => MAX_DATA,
        };
        if length == 0 || length > limit {
            return Err(invalid("Ailuros frame exceeds its kind's limit"));
        }
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Frame;
    type Error = io::Error;

    fn decode(&mut self, source: &mut BytesMut) -> io::Result<Option<Frame>> {
        if source.len() < HEADER_BYTES {
            return Ok(None);
        }
        let kind = Kind::try_from(source[0])?;
        let length = usize::try_from(u32::from_be_bytes(
            source[1..HEADER_BYTES].try_into().unwrap(),
        ))
        .map_err(|_| invalid("Ailuros frame length cannot be represented"))?;
        self.validate(kind, length)?;
        let total = HEADER_BYTES
            .checked_add(length)
            .ok_or_else(|| invalid("Ailuros frame length overflow"))?;
        if source.len() < total {
            source.reserve(total - source.len());
            return Ok(None);
        }
        let mut payload = source.split_to(total);
        payload.advance(HEADER_BYTES);
        Ok(Some(Frame {
            kind,
            payload: payload.freeze(),
        }))
    }
}

impl Encoder<Frame> for Codec {
    type Error = io::Error;

    fn encode(&mut self, frame: Frame, destination: &mut BytesMut) -> io::Result<()> {
        self.validate(frame.kind, frame.payload.len())?;
        let length = u32::try_from(frame.payload.len())
            .map_err(|_| invalid("Ailuros frame length overflow"))?;
        destination.reserve(HEADER_BYTES + frame.payload.len());
        destination.put_u8(frame.kind as u8);
        destination.put_u32(length);
        destination.extend_from_slice(&frame.payload);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio_util::codec::FramedRead;

    #[test]
    fn fixed_header_and_partial_reads_keep_frame_boundaries() {
        let mut codec = Codec::default();
        let mut encoded = BytesMut::new();
        let frame = Frame::new(Kind::Control, Bytes::from_static(b"{}"));
        codec.encode(frame.clone(), &mut encoded).unwrap();
        assert_eq!(&encoded[..], &[0, 0, 0, 0, 2, b'{', b'}']);
        let mut partial = BytesMut::new();
        for index in 0..encoded.len() - 1 {
            partial.extend_from_slice(&encoded[index..index + 1]);
            assert!(codec.decode(&mut partial).unwrap().is_none());
        }
        partial.extend_from_slice(&encoded[encoded.len() - 1..]);
        assert_eq!(codec.decode(&mut partial).unwrap(), Some(frame.clone()));
        assert!(partial.is_empty());
        codec.encode(frame.clone(), &mut partial).unwrap();
        codec.encode(frame.clone(), &mut partial).unwrap();
        assert_eq!(codec.decode(&mut partial).unwrap(), Some(frame.clone()));
        assert_eq!(codec.decode(&mut partial).unwrap(), Some(frame));
        assert!(codec.decode(&mut partial).unwrap().is_none());
    }

    #[test]
    fn invalid_headers_fail_without_allocating_their_announced_bodies() {
        for (phase, kind, length) in [
            (Phase::Authentication, 9, 1),
            (Phase::Authentication, Kind::Data as u8, 1),
            (Phase::Authentication, Kind::Metadata as u8, 1),
            (
                Phase::Authentication,
                Kind::Control as u8,
                MAX_AUTH_CONTROL as u32 + 1,
            ),
            (
                Phase::Streaming,
                Kind::Control as u8,
                MAX_CONTROL as u32 + 1,
            ),
            (
                Phase::Streaming,
                Kind::Metadata as u8,
                MAX_CONTROL as u32 + 1,
            ),
            (Phase::Streaming, Kind::Data as u8, MAX_DATA as u32 + 1),
            (Phase::Streaming, Kind::Data as u8, u32::MAX),
            (Phase::Streaming, Kind::Data as u8, 0),
        ] {
            let mut source = BytesMut::with_capacity(HEADER_BYTES);
            source.put_u8(kind);
            source.put_u32(length);
            let capacity = source.capacity();
            assert!(Codec::new(phase).decode(&mut source).is_err());
            assert_eq!(source.capacity(), capacity);
            assert_eq!(source.len(), HEADER_BYTES);
        }
    }

    #[test]
    fn encoding_limits_do_not_write_partial_rejected_frames() {
        let mut codec = Codec::default();
        let mut destination = BytesMut::from(&b"existing"[..]);
        assert!(
            codec
                .encode(
                    Frame::new(Kind::Data, Bytes::from_static(b"x")),
                    &mut destination
                )
                .is_err()
        );
        assert_eq!(&destination[..], b"existing");
        codec.set_phase(Phase::Streaming);
        let mut encoded = BytesMut::new();
        for kind in [Kind::Control, Kind::Metadata, Kind::Data] {
            let limit = if kind == Kind::Data {
                MAX_DATA
            } else {
                MAX_CONTROL
            };
            let frame = Frame::new(kind, vec![7; limit]);
            codec.encode(frame.clone(), &mut encoded).unwrap();
            assert_eq!(codec.decode(&mut encoded).unwrap(), Some(frame));
            assert!(
                codec
                    .encode(Frame::new(kind, vec![7; limit + 1]), &mut encoded)
                    .is_err()
            );
            assert!(encoded.is_empty());
        }
    }

    #[tokio::test]
    async fn cancelling_a_pending_read_preserves_partial_headers_and_rejects_truncation() {
        let (reader, mut writer) = tokio::io::duplex(64);
        let mut frames = FramedRead::with_capacity(reader, Codec::default(), HEADER_BYTES);
        writer.write_all(&[0, 0, 0]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), frames.next())
                .await
                .is_err()
        );
        writer.write_all(&[0, 2, b'{', b'}']).await.unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(1), frames.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(frame, Frame::new(Kind::Control, Bytes::from_static(b"{}")));
        writer.write_all(&[0, 0, 0, 0, 4, b'x']).await.unwrap();
        writer.shutdown().await.unwrap();
        assert!(frames.next().await.unwrap().is_err());
    }
}
