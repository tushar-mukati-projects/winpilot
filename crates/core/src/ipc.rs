use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    config::Config,
    error::Error,
    protocol::{Call, Reply},
};

pub const MAX_FRAME_BYTES: usize = 64 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToWorker {
    Init { config: Box<Config> },
    Call { id: u64, call: Call },
    Cancel { id: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FromWorker {
    Ready { version: String },
    Reply { id: u64, result: Box<Result<Reply, Error>> },
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
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("frame of {length} bytes is too large")));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClipboardAction, ClipboardArgs};

    #[test]
    fn frames_round_trip() {
        let mut buffer = Vec::new();
        let call =
            ToWorker::Call { id: 7, call: Call::Clipboard(ClipboardArgs { action: ClipboardAction::Get, text: None }) };
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
    fn oversized_header_is_rejected() {
        let header = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        assert!(read_frame::<_, ToWorker>(&mut header.as_slice()).is_err());
    }
}
