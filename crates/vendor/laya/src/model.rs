use crate::modernbert::ModernBert;
use anyhow::Result;
use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::ops::softmax;
use candle_nn::{Embedding, LayerNorm, Linear, Module, VarBuilder, embedding, layer_norm, linear};

use crate::config::EncoderConfig;

const LN_EPS: f64 = 1e-5;
const HEAD_DIM: usize = 64;
const MASK_FILL: f64 = -1e4;

struct HeadLayer {
    norm1: LayerNorm,
    norm2: LayerNorm,
    in_proj_w: Tensor,
    in_proj_b: Tensor,
    out_proj: Linear,
    linear1: Linear,
    linear2: Linear,
    n_heads: usize,
}

impl HeadLayer {
    fn load(vb: VarBuilder, d: usize, ff: usize) -> Result<Self> {
        let n_heads = std::cmp::max(1, d / HEAD_DIM);
        Ok(Self {
            norm1: layer_norm(d, LN_EPS, vb.pp("norm1"))?,
            norm2: layer_norm(d, LN_EPS, vb.pp("norm2"))?,
            in_proj_w: vb.get((3 * d, d), "self_attn.in_proj_weight")?,
            in_proj_b: vb.get(3 * d, "self_attn.in_proj_bias")?,
            out_proj: linear(d, d, vb.pp("self_attn.out_proj"))?,
            linear1: linear(d, ff, vb.pp("linear1"))?,
            linear2: linear(ff, d, vb.pp("linear2"))?,
            n_heads,
        })
    }

    fn attn(&self, x: &Tensor, pad_bias: &Tensor) -> Result<Tensor> {
        let (b, l, d) = x.dims3()?;
        let hd = d / self.n_heads;

        let qkv = x
            .broadcast_matmul(&self.in_proj_w.t()?)?
            .broadcast_add(&self.in_proj_b)?;
        let qkv = qkv
            .reshape((b, l, 3, self.n_heads, hd))?
            .permute((2, 0, 3, 1, 4))?
            .contiguous()?;
        let q = qkv.i(0)?;
        let k = qkv.i(1)?;
        let v = qkv.i(2)?;

        let scale = (hd as f64).powf(-0.5);
        let att =
            (q.contiguous()? * scale)?.matmul(&k.transpose(D::Minus2, D::Minus1)?.contiguous()?)?;
        let att = att.broadcast_add(pad_bias)?;
        let att = softmax(&att, D::Minus1)?;

        let out = att.matmul(&v.contiguous()?)?;
        let out = out.transpose(1, 2)?.reshape((b, l, d))?;
        Ok(self.out_proj.forward(&out)?)
    }

    fn forward(&self, x: &Tensor, pad_bias: &Tensor) -> Result<Tensor> {
        let h = self.attn(&self.norm1.forward(x)?, pad_bias)?;
        let x = (x + h)?;
        let f = self
            .linear2
            .forward(&self.linear1.forward(&self.norm2.forward(&x)?)?.relu()?)?;
        Ok((x + f)?)
    }
}

struct Scorer {
    norm: LayerNorm,
    fc1: Linear,
    fc2: Linear,
}

impl Scorer {
    fn load(vb: VarBuilder, d: usize) -> Result<Self> {
        Ok(Self {
            norm: layer_norm(d, LN_EPS, vb.pp("0"))?,
            fc1: linear(d, d, vb.pp("1"))?,
            fc2: linear(d, 1, vb.pp("3"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let x = self.fc1.forward(&self.norm.forward(x)?)?.gelu_erf()?;
        Ok(self.fc2.forward(&x)?)
    }
}

struct ActHead {
    fc1: Linear,
    fc2: Linear,
}

impl ActHead {
    fn load(vb: VarBuilder, d: usize, n_act: usize) -> Result<Self> {
        Ok(Self {
            fc1: linear(d + 4, 256, vb.pp("0"))?,
            fc2: linear(256, n_act, vb.pp("2"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        Ok(self.fc2.forward(&self.fc1.forward(x)?.gelu_erf()?)?)
    }
}

pub struct DecisionModel {
    encoder: ModernBert,
    type_emb: Embedding,
    layers: Vec<HeadLayer>,
    scorer: Scorer,
    act_head: ActHead,
    pub device: Device,
    pub dtype: DType,
}

pub struct Outputs {
    pub logits: Vec<Vec<f32>>,
    pub act: Vec<Vec<f32>>,
}

impl DecisionModel {
    pub fn load(
        vb: VarBuilder,
        enc_cfg: &EncoderConfig,
        head_layers: usize,
        n_act: usize,
        device: Device,
        dtype: DType,
    ) -> Result<Self> {
        let d = enc_cfg.hidden_size;
        let encoder = ModernBert::load(vb.clone(), &enc_cfg.to_candle())?;
        let type_emb = embedding(3, d, vb.pp("type_emb"))?;
        let mut layers = Vec::with_capacity(head_layers);
        for i in 0..head_layers {
            layers.push(HeadLayer::load(
                vb.pp(format!("head.layers.{i}")),
                d,
                4 * d,
            )?);
        }
        Ok(Self {
            encoder,
            type_emb,
            layers,
            scorer: Scorer::load(vb.pp("scorer"), d)?,
            act_head: ActHead::load(vb.pp("act_head"), d, n_act)?,
            device,
            dtype,
        })
    }

    pub fn forward(
        &self,
        input_ids: &Tensor,
        attention_mask: &Tensor,
        marker_pos: &Tensor,
        marker_mask: &[Vec<bool>],
        qtype: &Tensor,
    ) -> Result<Outputs> {
        let h = self.encoder.forward(input_ids, attention_mask)?;
        let (b, _l, d) = h.dims3()?;

        let te = self.type_emb.forward(qtype)?.unsqueeze(1)?;
        let mut h = h.broadcast_add(&te.to_dtype(h.dtype())?)?;

        let pad_bias = ((attention_mask.to_dtype(h.dtype())? - 1.0)? * 1e30)?
            .unsqueeze(1)?
            .unsqueeze(1)?;
        for layer in &self.layers {
            h = layer.forward(&h, &pad_bias)?;
        }

        let kmax = marker_pos.dim(1)?;
        let idx = marker_pos
            .unsqueeze(2)?
            .expand((b, kmax, d))?
            .contiguous()?;
        let m = h.gather(&idx, 1)?;

        let logits = self
            .scorer
            .forward(&m)?
            .squeeze(D::Minus1)?
            .to_dtype(DType::F32)?;
        let mut logits: Vec<Vec<f32>> = logits.to_vec2()?;
        for (row, mask) in logits.iter_mut().zip(marker_mask) {
            for (v, keep) in row.iter_mut().zip(mask) {
                if !keep {
                    *v = MASK_FILL as f32;
                }
            }
        }

        let feats: Vec<f32> = logits
            .iter()
            .zip(marker_mask)
            .flat_map(|(row, mask)| {
                let p = stable_softmax(row);
                let k = mask.iter().filter(|m| **m).count().max(2) as f32;
                let ent: f32 = -p.iter().map(|x| x * x.max(1e-9).ln()).sum::<f32>() / k.ln();
                let mut sorted = p.clone();
                sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
                let (t1, t2) = (sorted[0], sorted.get(1).copied().unwrap_or(0.0));
                [t1, t1 - t2, ent, k / 255.0]
            })
            .collect();
        let feats = Tensor::from_vec(feats, (b, 4), &self.device)?.to_dtype(h.dtype())?;
        let pooled = h.i((.., 0, ..))?;
        let act = self
            .act_head
            .forward(&Tensor::cat(&[&pooled, &feats], 1)?)?;
        let act = softmax(&act.to_dtype(DType::F32)?, D::Minus1)?.to_vec2()?;

        Ok(Outputs { logits, act })
    }
}

pub fn stable_softmax(z: &[f32]) -> Vec<f32> {
    let max = z.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let e: Vec<f32> = z.iter().map(|v| (v - max).exp()).collect();
    let s: f32 = e.iter().sum();
    e.into_iter().map(|v| v / s).collect()
}
