use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    config::Config,
    error::Error,
    protocol::{Call, Reply},
};

const MAX_FRAME_BYTES: usize = 64 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToWorker {
    Init {
        config: Box<Config>,
        epoch: u32,
        #[serde(default)]
        agent: String,
    },
    Call {
        id: u64,
        call: Call,
        deadline_ms: u64,
    },
    Cancel {
        id: u64,
    },
    Agent {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FromWorker {
    Ready {
        version: String,
        #[serde(default)]
        build: String,
    },
    Reply {
        id: u64,
        result: Box<Result<Reply, Error>>,
    },
}

pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, message: &T) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("frame of {} bytes is too large", body.len())));
    }
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

pub fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<Option<T>> {
    let Some(header) = read_header(reader)? else { return Ok(None) };
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("frame of {length} bytes is too large")));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_header<R: Read>(reader: &mut R) -> io::Result<Option<[u8; 4]>> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) => return partial_header(filled),
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(Some(header))
}

fn partial_header(filled: usize) -> io::Result<Option<[u8; 4]>> {
    if filled == 0 {
        return Ok(None);
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, format!("stream ended inside a frame header after {filled} bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ObserveArgs, ObserveMode};

    #[test]
    fn frames_round_trip() {
        let mut buffer = Vec::new();
        let clipboard = ObserveArgs { mode: ObserveMode::Clipboard, ..ObserveArgs::default() };
        let call = ToWorker::Call { id: 7, call: Call::Observe(clipboard), deadline_ms: 4000 };
        write_frame(&mut buffer, &call).unwrap();
        write_frame(&mut buffer, &ToWorker::Cancel { id: 7 }).unwrap();
        let mut reader = buffer.as_slice();
        assert_eq!(read_frame::<_, ToWorker>(&mut reader).unwrap(), Some(call));
        assert_eq!(read_frame::<_, ToWorker>(&mut reader).unwrap(), Some(ToWorker::Cancel { id: 7 }));
        assert_eq!(read_frame::<_, ToWorker>(&mut reader).unwrap(), None);
    }

    #[test]
    fn truncated_body_is_an_error() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &ToWorker::Cancel { id: 1 }).unwrap();
        buffer.pop();
        assert!(read_frame::<_, ToWorker>(&mut buffer.as_slice()).is_err());
    }

    #[test]
    fn partial_header_is_an_error_not_a_clean_end() {
        for length in 1..4 {
            let bytes = vec![1u8; length];
            let error = read_frame::<_, ToWorker>(&mut bytes.as_slice()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{length}");
        }
        assert_eq!(read_frame::<_, ToWorker>(&mut [].as_slice()).unwrap(), None);
    }

    #[test]
    fn header_split_across_reads_is_reassembled() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &ToWorker::Cancel { id: 9 }).unwrap();
        let mut reader = buffer[..2].chain(&buffer[2..]);
        assert_eq!(read_frame::<_, ToWorker>(&mut reader).unwrap(), Some(ToWorker::Cancel { id: 9 }));
    }

    #[test]
    fn zero_length_frame_is_invalid_json() {
        let error = read_frame::<_, ToWorker>(&mut [0u8, 0, 0, 0].as_slice()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn oversized_header_is_rejected() {
        let header = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        assert!(read_frame::<_, ToWorker>(&mut header.as_slice()).is_err());
    }

    #[test]
    fn a_ready_without_a_build_id_still_parses_so_the_mismatch_can_be_reported() {
        let ready: FromWorker = serde_json::from_str(r#"{"ready":{"version":"0.1.0"}}"#).unwrap();
        assert_eq!(ready, FromWorker::Ready { version: "0.1.0".to_string(), build: String::new() });
    }
}
