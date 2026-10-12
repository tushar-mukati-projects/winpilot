use candle_core::{D, Device, IndexOp, Result, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, Module, VarBuilder, embedding, layer_norm, linear, linear_no_bias};
use candle_transformers::models::whisper::Config;

const LAYER_NORM_EPS: f64 = 1e-5;

struct Attention {
    query: Linear,
    key: Linear,
    value: Linear,
    out: Linear,
    heads: usize,
    cache: Option<(Tensor, Tensor)>,
}

impl Attention {
    fn load(width: usize, heads: usize, weights: &VarBuilder) -> Result<Self> {
        Ok(Self {
            query: linear(width, width, weights.pp("q_proj"))?,
            key: linear_no_bias(width, width, weights.pp("k_proj"))?,
            value: linear(width, width, weights.pp("v_proj"))?,
            out: linear(width, width, weights.pp("out_proj"))?,
            heads,
            cache: None,
        })
    }

    fn attend_to_self(&mut self, x: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let key = self.key.forward(x)?;
        let value = self.value.forward(x)?;
        let (key, value) = match self.cache.take() {
            Some((old_key, old_value)) => (Tensor::cat(&[&old_key, &key], 1)?, Tensor::cat(&[&old_value, &value], 1)?),
            None => (key, value),
        };
        self.cache = Some((key.clone(), value.clone()));
        self.attend(&self.query.forward(x)?, &key, &value, mask)
    }

    fn attend_to_audio(&mut self, x: &Tensor, features: &Tensor) -> Result<Tensor> {
        if self.cache.is_none() {
            self.cache = Some((self.key.forward(features)?, self.value.forward(features)?));
        }
        let Some((key, value)) = &self.cache else { return Err(candle_core::Error::Msg("no audio keys".into())) };
        self.attend(&self.query.forward(x)?, key, value, None)
    }

    fn attend(&self, query: &Tensor, key: &Tensor, value: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let (_, _, width) = query.dims3()?;
        let scale = ((width / self.heads) as f64).powf(-0.25);
        let query = (self.split_heads(query)? * scale)?;
        let key = (self.split_heads(key)?.transpose(2, 3)? * scale)?;
        let value = self.split_heads(value)?.contiguous()?;
        let mut scores = query.contiguous()?.matmul(&key.contiguous()?)?;
        if let Some(mask) = mask {
            scores = scores.broadcast_add(mask)?;
        }
        let weights = candle_nn::ops::softmax_last_dim(&scores)?;
        let mixed = weights.matmul(&value)?.transpose(1, 2)?.flatten_from(2)?;
        self.out.forward(&mixed)
    }

    fn split_heads(&self, x: &Tensor) -> Result<Tensor> {
        let (batch, length, width) = x.dims3()?;
        x.reshape((batch, length, self.heads, width / self.heads))?.transpose(1, 2)
    }
}

struct Block {
    self_attention: Attention,
    self_norm: LayerNorm,
    cross_attention: Attention,
    cross_norm: LayerNorm,
    expand: Linear,
    contract: Linear,
    mlp_norm: LayerNorm,
}

impl Block {
    fn load(width: usize, heads: usize, weights: &VarBuilder) -> Result<Self> {
        Ok(Self {
            self_attention: Attention::load(width, heads, &weights.pp("self_attn"))?,
            self_norm: layer_norm(width, LAYER_NORM_EPS, weights.pp("self_attn_layer_norm"))?,
            cross_attention: Attention::load(width, heads, &weights.pp("encoder_attn"))?,
            cross_norm: layer_norm(width, LAYER_NORM_EPS, weights.pp("encoder_attn_layer_norm"))?,
            expand: linear(width, width * 4, weights.pp("fc1"))?,
            contract: linear(width * 4, width, weights.pp("fc2"))?,
            mlp_norm: layer_norm(width, LAYER_NORM_EPS, weights.pp("final_layer_norm"))?,
        })
    }

    fn forward(&mut self, x: &Tensor, features: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let x = (x + self.self_attention.attend_to_self(&self.self_norm.forward(x)?, mask)?)?;
        let x = (&x + self.cross_attention.attend_to_audio(&self.cross_norm.forward(&x)?, features)?)?;
        let hidden = self.expand.forward(&self.mlp_norm.forward(&x)?)?.gelu_erf()?;
        x + self.contract.forward(&hidden)?
    }

    fn reset(&mut self) {
        self.self_attention.cache = None;
        self.cross_attention.cache = None;
    }
}

pub(crate) struct CachedDecoder {
    token_embedding: Embedding,
    positions: Tensor,
    blocks: Vec<Block>,
    norm: LayerNorm,
    offset: usize,
}

impl CachedDecoder {
    pub(crate) fn load(weights: &VarBuilder, config: &Config) -> Result<Self> {
        let weights = weights.pp("model.decoder");
        let width = config.d_model;
        let blocks = (0..config.decoder_layers)
            .map(|layer| Block::load(width, config.decoder_attention_heads, &weights.pp(format!("layers.{layer}"))))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            token_embedding: embedding(config.vocab_size, width, weights.pp("embed_tokens"))?,
            positions: weights.get((config.max_target_positions, width), "embed_positions.weight")?,
            blocks,
            norm: layer_norm(width, LAYER_NORM_EPS, weights.pp("layer_norm"))?,
            offset: 0,
        })
    }

    pub(crate) fn reset(&mut self) {
        self.offset = 0;
        self.blocks.iter_mut().for_each(Block::reset);
    }

    pub(crate) fn capacity_left(&self) -> usize {
        self.positions.dim(0).map_or(0, |total| total.saturating_sub(self.offset))
    }

    pub(crate) fn forward(&mut self, tokens: &[u32], features: &Tensor) -> Result<Tensor> {
        let input = Tensor::new(tokens, &Device::Cpu)?.unsqueeze(0)?;
        let positions = self.positions.narrow(0, self.offset, tokens.len())?;
        let mut x = self.token_embedding.forward(&input)?.broadcast_add(&positions)?;
        let mask = causal_mask(tokens.len(), self.offset)?;
        for block in &mut self.blocks {
            x = block.forward(&x, features, mask.as_ref())?;
        }
        self.offset += tokens.len();
        self.norm.forward(&x)
    }

    pub(crate) fn logits_at(&self, hidden: &Tensor, position: usize) -> Result<Vec<f32>> {
        let row = hidden.i((0, position))?.unsqueeze(0)?;
        row.matmul(&self.token_embedding.embeddings().t()?)?.squeeze(0)?.to_vec1::<f32>()
    }
}

fn causal_mask(length: usize, offset: usize) -> Result<Option<Tensor>> {
    if length < 2 {
        return Ok(None);
    }
    let total = offset + length;
    let values: Vec<f32> = (0..length)
        .flat_map(|row| (0..total).map(move |column| if column > offset + row { f32::NEG_INFINITY } else { 0.0 }))
        .collect();
    Tensor::from_vec(values, (length, total), &Device::Cpu).map(Some)
}

pub(crate) fn last_position(hidden: &Tensor) -> Result<usize> {
    Ok(hidden.dim(D::Minus2)?.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tokens_need_no_mask() {
        assert!(causal_mask(1, 5).unwrap().is_none());
    }

    #[test]
    fn prompt_mask_hides_future_positions() {
        let mask = causal_mask(3, 0).unwrap().unwrap().to_vec2::<f32>().unwrap();
        assert_eq!(mask[0], vec![0.0, f32::NEG_INFINITY, f32::NEG_INFINITY]);
        assert_eq!(mask[1], vec![0.0, 0.0, f32::NEG_INFINITY]);
        assert_eq!(mask[2], vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn mask_after_cached_tokens_sees_the_cache() {
        let mask = causal_mask(2, 3).unwrap().unwrap().to_vec2::<f32>().unwrap();
        assert_eq!(mask[0], vec![0.0, 0.0, 0.0, 0.0, f32::NEG_INFINITY]);
        assert_eq!(mask[1], vec![0.0; 5]);
    }
}
