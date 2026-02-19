//! 10K-parameter MLP neural model.
//!
//! Flat input projection + hidden MLP + autoregressive decoder.
//! DIM=32, FFN=32, vocab=64.

use crate::field::fixed::{Fixed, RawAccum};
use crate::field::goldilocks::Goldilocks;
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, MAX_NODES, WORDS_PER_NODE};

pub const DIM: usize = 32;
pub const FFN: usize = 32;
pub const VOCAB: usize = 64;
pub const MAX_OUTPUT: usize = 16;
pub const INPUT_FLAT: usize = MAX_NODES * WORDS_PER_NODE; // 128

/// Total parameters: 10,400.
///   input_proj:     128 * 32 = 4,096
///   input_bias:     32
///   hidden_w:       32 * 32  = 1,024
///   hidden_bias:    32
///   dec_hidden:     96 * 32  = 3,072
///   dec_hidden_bias: 32
///   dec_output:     32 * 64  = 2,048
///   dec_output_bias: 64
pub const PARAM_COUNT: usize = 10_400;

struct Scratch {
    projected: Vec<Fixed>,
    hidden: Vec<Fixed>,
    dec_h: Vec<Fixed>,
    dec_out: Vec<Fixed>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            projected: vec![Fixed::ZERO; DIM],
            hidden: vec![Fixed::ZERO; DIM],
            dec_h: vec![Fixed::ZERO; FFN],
            dec_out: vec![Fixed::ZERO; VOCAB],
        }
    }
}

pub struct NeuralModel {
    pub input_proj: Vec<Fixed>,      // [INPUT_FLAT * DIM]
    pub input_bias: Vec<Fixed>,      // [DIM]
    pub hidden_w: Vec<Fixed>,        // [DIM * DIM]
    pub hidden_bias: Vec<Fixed>,     // [DIM]
    pub dec_hidden: Vec<Fixed>,      // [(DIM + VOCAB) * FFN]
    pub dec_hidden_bias: Vec<Fixed>, // [FFN]
    pub dec_output: Vec<Fixed>,      // [FFN * VOCAB]
    pub dec_output_bias: Vec<Fixed>, // [VOCAB]
    scratch: Scratch,
}

impl NeuralModel {
    pub fn zeros() -> Self {
        Self {
            input_proj: vec![Fixed::ZERO; INPUT_FLAT * DIM],
            input_bias: vec![Fixed::ZERO; DIM],
            hidden_w: vec![Fixed::ZERO; DIM * DIM],
            hidden_bias: vec![Fixed::ZERO; DIM],
            dec_hidden: vec![Fixed::ZERO; (DIM + VOCAB) * FFN],
            dec_hidden_bias: vec![Fixed::ZERO; FFN],
            dec_output: vec![Fixed::ZERO; FFN * VOCAB],
            dec_output_bias: vec![Fixed::ZERO; VOCAB],
            scratch: Scratch::new(),
        }
    }

    pub fn forward(&mut self, block: &TIRBlock) -> Vec<u64> {
        // Zero-weight fast path
        if self.input_proj.iter().all(|w| w.0 == Goldilocks(0)) {
            return Vec::new();
        }

        let seq_len = block.node_count.max(1);
        let s = &mut self.scratch;

        // 1. Flatten + project: all nodes → [DIM] + bias + ReLU
        for d in 0..DIM {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.input_bias[d]);
            for n in 0..seq_len {
                let node_start = n * WORDS_PER_NODE;
                for w in 0..WORDS_PER_NODE {
                    let input = Fixed::from_raw(Goldilocks::from_u64(block.nodes[node_start + w]));
                    acc.add_prod(input, self.input_proj[(n * WORDS_PER_NODE + w) * DIM + d]);
                }
            }
            s.projected[d] = acc.finish().relu();
        }

        // 2. Hidden layer: [DIM] → [DIM] + bias + ReLU
        for d in 0..DIM {
            let mut acc = RawAccum::zero();
            acc.add_bias(self.hidden_bias[d]);
            for j in 0..DIM {
                acc.add_prod(s.projected[j], self.hidden_w[j * DIM + d]);
            }
            s.hidden[d] = acc.finish().relu();
        }

        // 3. Autoregressive decoder
        let mut output = Vec::with_capacity(MAX_OUTPUT);
        let mut prev_out = vec![Fixed::ZERO; VOCAB];

        for _ in 0..MAX_OUTPUT {
            // Decoder hidden: [DIM + VOCAB] → [FFN] + bias + ReLU
            for fh in 0..FFN {
                let mut acc = RawAccum::zero();
                acc.add_bias(self.dec_hidden_bias[fh]);
                for d in 0..DIM {
                    acc.add_prod(s.hidden[d], self.dec_hidden[d * FFN + fh]);
                }
                for d in 0..VOCAB {
                    acc.add_prod(prev_out[d], self.dec_hidden[(DIM + d) * FFN + fh]);
                }
                s.dec_h[fh] = acc.finish().relu();
            }

            // Decoder output: [FFN] → [VOCAB] + bias
            for d in 0..VOCAB {
                let mut acc = RawAccum::zero();
                acc.add_bias(self.dec_output_bias[d]);
                for fh in 0..FFN {
                    acc.add_prod(s.dec_h[fh], self.dec_output[fh * VOCAB + d]);
                }
                s.dec_out[d] = acc.finish();
            }

            // Argmax over VOCAB positions
            let mut best_val = s.dec_out[0].to_f64();
            let mut best_idx = 0u64;
            for (i, x) in s.dec_out.iter().enumerate().skip(1) {
                let v = x.to_f64();
                if v > best_val {
                    best_val = v;
                    best_idx = i as u64;
                }
            }

            if best_idx == 0 {
                break;
            }
            output.push(best_idx);
            prev_out.copy_from_slice(&s.dec_out);
        }

        output
    }

    pub fn to_weight_vec(&self) -> Vec<Fixed> {
        let mut v = Vec::with_capacity(PARAM_COUNT);
        v.extend_from_slice(&self.input_proj);
        v.extend_from_slice(&self.input_bias);
        v.extend_from_slice(&self.hidden_w);
        v.extend_from_slice(&self.hidden_bias);
        v.extend_from_slice(&self.dec_hidden);
        v.extend_from_slice(&self.dec_hidden_bias);
        v.extend_from_slice(&self.dec_output);
        v.extend_from_slice(&self.dec_output_bias);
        v
    }

    pub fn from_weight_vec(w: &[Fixed]) -> Self {
        let mut i = 0;
        let mut take = |n: usize| -> Vec<Fixed> {
            let slice = w[i..i + n].to_vec();
            i += n;
            slice
        };

        let input_proj = take(INPUT_FLAT * DIM);
        let input_bias = take(DIM);
        let hidden_w = take(DIM * DIM);
        let hidden_bias = take(DIM);
        let dec_hidden = take((DIM + VOCAB) * FFN);
        let dec_hidden_bias = take(FFN);
        let dec_output = take(FFN * VOCAB);
        let dec_output_bias = take(VOCAB);

        Self {
            input_proj,
            input_bias,
            hidden_w,
            hidden_bias,
            dec_hidden,
            dec_hidden_bias,
            dec_output,
            dec_output_bias,
            scratch: Scratch::new(),
        }
    }

    pub fn weight_count(&self) -> usize {
        PARAM_COUNT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tir::encode::CONTEXT_SIZE;

    #[test]
    fn zeros_model_produces_empty() {
        let mut model = NeuralModel::zeros();
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 3,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 3,
        };
        assert!(model.forward(&block).is_empty());
    }

    #[test]
    fn weight_vec_roundtrip() {
        let model = NeuralModel::zeros();
        let weights = model.to_weight_vec();
        assert_eq!(weights.len(), PARAM_COUNT);
        let restored = NeuralModel::from_weight_vec(&weights);
        let w2 = restored.to_weight_vec();
        assert_eq!(weights, w2);
    }

    #[test]
    fn forward_deterministic() {
        let mut model = NeuralModel::from_weight_vec(&vec![Fixed::from_f64(0.01); PARAM_COUNT]);
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 2,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 2,
        };
        let out1 = model.forward(&block);
        let out2 = model.forward(&block);
        assert_eq!(out1, out2);
    }

    #[test]
    fn param_count_matches() {
        let model = NeuralModel::zeros();
        let vec = model.to_weight_vec();
        assert_eq!(vec.len(), PARAM_COUNT);
        assert_eq!(vec.len(), 10_400);
    }
}
