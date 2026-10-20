use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;

use candle_core::{DType, Device, Tensor};
use winpilot_core::{Context, Error, Result};

use crate::tensor_names::candle_name;

const MAGIC: u32 = 0x6767_6d6c;
const MAX_MELS: usize = 256;
const MAX_FFT_BINS: usize = 1_024;
const MAX_TOKENS: usize = 200_000;
const MAX_TOKEN_BYTES: usize = 1_024;
const MAX_NAME_BYTES: usize = 256;
const MAX_DIMS: usize = 4;
const MAX_ELEMENTS: usize = 1 << 30;
const MAX_HPARAM: usize = 1 << 20;
const F32_BYTES: usize = size_of::<f32>();
const READ_BUFFER_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hparams {
    pub vocab: usize,
    pub audio_ctx: usize,
    pub audio_state: usize,
    pub audio_head: usize,
    pub audio_layer: usize,
    pub text_ctx: usize,
    pub text_state: usize,
    pub text_head: usize,
    pub text_layer: usize,
    pub mels: usize,
}

pub(crate) struct GgmlModel {
    pub hparams: Hparams,
    pub filters: Vec<f32>,
    pub tokens: Vec<Vec<u8>>,
    pub tensors: HashMap<String, Tensor>,
}

pub(crate) fn read_model(path: &Path) -> Result<GgmlModel> {
    let file = File::open(path).context(&format!("opening the speech model {}", path.display()))?;
    let mut reader = ModelReader { inner: BufReader::with_capacity(READ_BUFFER_BYTES, file) };
    if reader.u32()? != MAGIC {
        return Err(Error::unsupported(format!("{} is not a whisper.cpp ggml model", path.display())));
    }
    let hparams = reader.hparams()?;
    let filters = reader.filters(hparams.mels)?;
    let tokens = reader.tokens()?;
    let tensors = reader.tensors()?;
    Ok(GgmlModel { hparams, filters, tokens, tensors })
}

struct ModelReader<R> {
    inner: R,
}

impl<R: Read> ModelReader<R> {
    fn bytes(&mut self, count: usize) -> Result<Vec<u8>> {
        let mut buffer = vec![0; count];
        self.inner.read_exact(&mut buffer).map_err(|error| truncated(&error))?;
        Ok(buffer)
    }

    fn u32(&mut self) -> Result<u32> {
        let mut buffer = [0; 4];
        self.inner.read_exact(&mut buffer).map_err(|error| truncated(&error))?;
        Ok(u32::from_le_bytes(buffer))
    }

    fn count(&mut self, limit: usize, what: &str) -> Result<usize> {
        let value = self.u32()? as usize;
        if value > limit {
            return Err(Error::unsupported(format!("the speech model has an implausible {what} ({value})")));
        }
        Ok(value)
    }

    fn hparams(&mut self) -> Result<Hparams> {
        let mut next = |what| self.count(MAX_HPARAM, what);
        let hparams = Hparams {
            vocab: next("vocabulary size")?,
            audio_ctx: next("audio context")?,
            audio_state: next("audio width")?,
            audio_head: next("audio heads")?,
            audio_layer: next("audio layers")?,
            text_ctx: next("text context")?,
            text_state: next("text width")?,
            text_head: next("text heads")?,
            text_layer: next("text layers")?,
            mels: next("mel bands")?,
        };
        self.u32()?;
        Ok(hparams)
    }

    fn filters(&mut self, mels: usize) -> Result<Vec<f32>> {
        let rows = self.count(MAX_MELS, "mel filter count")?;
        let columns = self.count(MAX_FFT_BINS, "mel filter width")?;
        if rows != mels {
            return Err(Error::unsupported("the speech model's mel filters do not match its mel bands"));
        }
        let raw = self.bytes(rows * columns * F32_BYTES)?;
        Ok(raw.as_chunks::<F32_BYTES>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)).collect())
    }

    fn tokens(&mut self) -> Result<Vec<Vec<u8>>> {
        let count = self.count(MAX_TOKENS, "token count")?;
        (0..count)
            .map(|_| {
                let length = self.count(MAX_TOKEN_BYTES, "token length")?;
                self.bytes(length)
            })
            .collect()
    }

    fn tensors(&mut self) -> Result<HashMap<String, Tensor>> {
        let mut tensors = HashMap::new();
        while let Some(dims) = self.tensor_dims()? {
            let (name, tensor) = self.tensor(dims)?;
            if let Some(mapped) = candle_name(&name) {
                tensors.insert(mapped, tensor);
            }
        }
        Ok(tensors)
    }

    fn tensor_dims(&mut self) -> Result<Option<usize>> {
        let mut buffer = [0; 4];
        match self.inner.read(&mut buffer[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(error) => return Err(truncated(&error)),
        }
        self.inner.read_exact(&mut buffer[1..]).map_err(|error| truncated(&error))?;
        let dims = u32::from_le_bytes(buffer) as usize;
        if dims == 0 || dims > MAX_DIMS {
            return Err(Error::unsupported(format!("the speech model has a tensor with {dims} dimensions")));
        }
        Ok(Some(dims))
    }

    fn tensor(&mut self, dims: usize) -> Result<(String, Tensor)> {
        let name_length = self.count(MAX_NAME_BYTES, "tensor name length")?;
        let dtype = tensor_dtype(self.u32()?)?;
        let mut shape = (0..dims).map(|_| self.count(MAX_ELEMENTS, "tensor dimension")).collect::<Result<Vec<_>>>()?;
        shape.reverse();
        let name = String::from_utf8(self.bytes(name_length)?)
            .map_err(|_| Error::unsupported("the speech model has a tensor name that is not UTF-8"))?;
        let elements = element_count(&shape)?;
        let raw = self.bytes(elements * dtype.size_in_bytes())?;
        let tensor = Tensor::from_raw_buffer(&raw, dtype, &shape, &Device::Cpu)
            .and_then(|tensor| tensor.to_dtype(DType::F32))
            .context(&format!("reading the model tensor {name}"))?;
        let tensor = if name.rsplit_once('.').is_some_and(|(_, last)| last == "bias") {
            tensor.flatten_all().context("flattening a bias")?
        } else {
            tensor
        };
        Ok((name, tensor))
    }
}

fn tensor_dtype(code: u32) -> Result<DType> {
    match code {
        0 => Ok(DType::F32),
        1 => Ok(DType::F16),
        _ => Err(Error::unsupported(
            "quantized speech models are not supported; install a plain model with scripts\\install.ps1 -SpeechModel base",
        )),
    }
}

fn element_count(shape: &[usize]) -> Result<usize> {
    shape
        .iter()
        .try_fold(1_usize, |total, dim| total.checked_mul(*dim))
        .filter(|total| *total <= MAX_ELEMENTS)
        .ok_or_else(|| Error::unsupported("the speech model has an implausibly large tensor"))
}

fn truncated(error: &std::io::Error) -> Error {
    if error.kind() == ErrorKind::UnexpectedEof {
        return Error::unsupported("the speech model file is truncated; reinstall it with scripts\\install.ps1");
    }
    Error::internal(format!("reading the speech model: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|value| value.to_le_bytes()).collect()
    }

    fn reader(bytes: Vec<u8>) -> ModelReader<std::io::Cursor<Vec<u8>>> {
        ModelReader { inner: std::io::Cursor::new(bytes) }
    }

    #[test]
    fn reads_an_f32_tensor_in_numpy_order() {
        let mut bytes = le(&[2, 4, 0, 3, 2]);
        bytes.extend_from_slice(b"x.wt");
        for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let mut model = reader(bytes);
        let dims = model.tensor_dims().unwrap().unwrap();
        let (name, tensor) = model.tensor(dims).unwrap();
        assert_eq!(name, "x.wt");
        assert_eq!(tensor.dims(), &[2, 3]);
        assert_eq!(tensor.to_vec2::<f32>().unwrap(), vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
        assert_eq!(model.tensor_dims().unwrap(), None);
    }

    #[test]
    fn biases_are_flattened() {
        let mut bytes = le(&[2, 6, 0, 1, 2]);
        bytes.extend_from_slice(b"c.bias");
        bytes.extend_from_slice(&le(&[0, 0]));
        let mut model = reader(bytes);
        let dims = model.tensor_dims().unwrap().unwrap();
        let (_, tensor) = model.tensor(dims).unwrap();
        assert_eq!(tensor.dims(), &[2]);
    }

    #[test]
    fn quantized_and_truncated_tensors_are_refused() {
        let mut quantized = reader(le(&[1, 1, 8, 4]));
        let dims = quantized.tensor_dims().unwrap().unwrap();
        assert!(quantized.tensor(dims).unwrap_err().message.contains("quantized"));
        let mut short = reader(le(&[1, 1, 0, 4, 0x61]));
        let dims = short.tensor_dims().unwrap().unwrap();
        assert!(short.tensor(dims).unwrap_err().message.contains("truncated"));
    }

    #[test]
    fn implausible_counts_are_refused() {
        assert!(reader(le(&[5])).tensor_dims().is_err());
        assert!(reader(le(&[MAX_TOKENS as u32 + 1])).tokens().is_err());
        assert!(element_count(&[1 << 16, 1 << 16]).is_err());
        assert_eq!(element_count(&[3, 4]).unwrap(), 12);
    }

    #[test]
    fn tokens_are_raw_bytes() {
        let mut bytes = le(&[2, 1]);
        bytes.push(b'a');
        bytes.extend_from_slice(&le(&[2]));
        bytes.extend_from_slice(&[0xE0, 0xA4]);
        assert_eq!(reader(bytes).tokens().unwrap(), vec![b"a".to_vec(), vec![0xE0, 0xA4]]);
    }
}
