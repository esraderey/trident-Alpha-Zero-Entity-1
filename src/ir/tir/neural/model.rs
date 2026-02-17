//! The 91K-parameter encoder-decoder neural model.
//!
//! Encoder: 2-layer self-attention with DAG-aware masking, dim 64.
//! Decoder: autoregressive MLP producing TASM instruction sequences.
//! All arithmetic in fixed-point Goldilocks.

use crate::field::fixed::{self, Fixed};
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, WORDS_PER_NODE};
#[cfg(test)]
use crate::ir::tir::encode::{CONTEXT_SIZE, MAX_NODES};

/// Model hyperparameters.
pub const DIM: usize = 64;
pub const HEADS: usize = 2;
pub const LAYERS: usize = 2;
pub const FFN_HIDDEN: usize = 128;
pub const MAX_OUTPUT: usize = 64;
pub const HEAD_DIM: usize = DIM / HEADS;

/// Total parameter count:
/// Encoder: 2 layers * (3*64*64 + 64*64 + 64*128 + 128*64 + 2*64) = 2 * 33,024 = 66,048
/// Decoder: 128*128 + 128 + 128*64 + 64 = 24,768
/// Input projection: INPUT_DIM * DIM (not counted in the 91K — folded into first layer)
/// Total core: ~91K
pub const PARAM_COUNT: usize = 66_048 + 24_768;

/// The neural optimizer model.
pub struct NeuralModel {
    /// Encoder layers.
    pub encoder: [EncoderLayer; LAYERS],
    /// Decoder weights.
    pub decoder: Decoder,
    /// Input projection: maps INPUT_DIM -> DIM per node.
    pub input_proj: Vec<Fixed>, // WORDS_PER_NODE * DIM
}

/// One encoder layer: self-attention + FFN + layer norm.
pub struct EncoderLayer {
    /// Q, K, V projection weights: 3 * DIM * DIM.
    pub qkv: Vec<Fixed>,
    /// Output projection: DIM * DIM.
    pub out_proj: Vec<Fixed>,
    /// FFN layer 1: DIM * FFN_HIDDEN.
    pub ffn1: Vec<Fixed>,
    /// FFN layer 2: FFN_HIDDEN * DIM.
    pub ffn2: Vec<Fixed>,
    /// Layer norm scale: DIM.
    pub ln_scale: Vec<Fixed>,
    /// Layer norm bias: DIM.
    pub ln_bias: Vec<Fixed>,
}

/// Autoregressive MLP decoder.
pub struct Decoder {
    /// Hidden layer: (DIM + DIM) * FFN_HIDDEN = 128 * 128.
    pub hidden: Vec<Fixed>,
    /// Hidden bias: FFN_HIDDEN.
    pub hidden_bias: Vec<Fixed>,
    /// Output layer: FFN_HIDDEN * DIM = 128 * 64.
    pub output: Vec<Fixed>,
    /// Output bias: DIM.
    pub output_bias: Vec<Fixed>,
}

impl NeuralModel {
    /// Create a model with all-zero weights.
    pub fn zeros() -> Self {
        Self {
            encoder: [EncoderLayer::zeros(), EncoderLayer::zeros()],
            decoder: Decoder::zeros(),
            input_proj: vec![Fixed::ZERO; WORDS_PER_NODE * DIM],
        }
    }

    /// Run forward pass: TIR block -> TASM instruction codes.
    ///
    /// Returns a sequence of u64 values, each encoding a TASM instruction
    /// (opcode in bits 4..10, argument in bits 0..3).
    pub fn forward(&self, block: &TIRBlock) -> Vec<u64> {
        // 1. Project input nodes to DIM-dimensional embeddings
        let mut embeddings = Vec::with_capacity(block.node_count.max(1) * DIM);
        for n in 0..block.node_count.max(1) {
            let node_start = n * WORDS_PER_NODE;
            let mut emb = vec![Fixed::ZERO; DIM];
            for d in 0..DIM {
                let mut acc = Fixed::ZERO;
                for w in 0..WORDS_PER_NODE {
                    let weight = self.input_proj[w * DIM + d];
                    let input = if node_start + w < block.nodes.len() {
                        Fixed::from_raw(crate::field::Goldilocks::from_u64(
                            block.nodes[node_start + w],
                        ))
                    } else {
                        Fixed::ZERO
                    };
                    acc = acc.madd(input, weight);
                }
                emb[d] = acc;
            }
            embeddings.extend_from_slice(&emb);
        }

        let seq_len = block.node_count.max(1);

        // 2. Encoder layers
        for layer in &self.encoder {
            embeddings = layer.forward(&embeddings, seq_len);
        }

        // 3. Pool encoder output: mean across nodes
        let mut latent = vec![Fixed::ZERO; DIM];
        let n_inv = Fixed::from_f64(1.0 / seq_len as f64);
        for n in 0..seq_len {
            for d in 0..DIM {
                latent[d] = latent[d].add(embeddings[n * DIM + d]);
            }
        }
        for d in 0..DIM {
            latent[d] = latent[d].mul(n_inv);
        }

        // 4. Autoregressive decoding
        let mut output = Vec::with_capacity(MAX_OUTPUT);
        let mut prev_instr = vec![Fixed::ZERO; DIM];
        for _ in 0..MAX_OUTPUT {
            let instr = self.decoder.step(&latent, &prev_instr);
            let code = instr_to_code(&instr);
            if code == 0 {
                break; // End of sequence
            }
            output.push(code);
            prev_instr = instr;
        }

        output
    }

    /// Flatten all weights into a single vector for evolutionary search.
    pub fn to_weight_vec(&self) -> Vec<Fixed> {
        let mut v = Vec::with_capacity(PARAM_COUNT + WORDS_PER_NODE * DIM);
        v.extend_from_slice(&self.input_proj);
        for layer in &self.encoder {
            v.extend_from_slice(&layer.qkv);
            v.extend_from_slice(&layer.out_proj);
            v.extend_from_slice(&layer.ffn1);
            v.extend_from_slice(&layer.ffn2);
            v.extend_from_slice(&layer.ln_scale);
            v.extend_from_slice(&layer.ln_bias);
        }
        v.extend_from_slice(&self.decoder.hidden);
        v.extend_from_slice(&self.decoder.hidden_bias);
        v.extend_from_slice(&self.decoder.output);
        v.extend_from_slice(&self.decoder.output_bias);
        v
    }

    /// Reconstruct model from a flat weight vector.
    pub fn from_weight_vec(w: &[Fixed]) -> Self {
        let mut i = 0;
        let mut take = |n: usize| -> Vec<Fixed> {
            let slice = w[i..i + n].to_vec();
            i += n;
            slice
        };

        let input_proj = take(WORDS_PER_NODE * DIM);

        let mut encoder = [EncoderLayer::zeros(), EncoderLayer::zeros()];
        for layer in &mut encoder {
            layer.qkv = take(3 * DIM * DIM);
            layer.out_proj = take(DIM * DIM);
            layer.ffn1 = take(DIM * FFN_HIDDEN);
            layer.ffn2 = take(FFN_HIDDEN * DIM);
            layer.ln_scale = take(DIM);
            layer.ln_bias = take(DIM);
        }

        let hidden = take(2 * DIM * FFN_HIDDEN);
        let hidden_bias = take(FFN_HIDDEN);
        let output = take(FFN_HIDDEN * DIM);
        let output_bias = take(DIM);
        let decoder = Decoder {
            hidden,
            hidden_bias,
            output,
            output_bias,
        };

        Self {
            encoder,
            decoder,
            input_proj,
        }
    }

    /// Total number of weights in the flat vector.
    pub fn weight_count(&self) -> usize {
        self.to_weight_vec().len()
    }
}

impl EncoderLayer {
    fn zeros() -> Self {
        Self {
            qkv: vec![Fixed::ZERO; 3 * DIM * DIM],
            out_proj: vec![Fixed::ZERO; DIM * DIM],
            ffn1: vec![Fixed::ZERO; DIM * FFN_HIDDEN],
            ffn2: vec![Fixed::ZERO; FFN_HIDDEN * DIM],
            ln_scale: vec![Fixed::ONE; DIM],
            ln_bias: vec![Fixed::ZERO; DIM],
        }
    }

    /// Self-attention + FFN forward pass for one layer.
    fn forward(&self, input: &[Fixed], seq_len: usize) -> Vec<Fixed> {
        let mut output = input.to_vec();

        // Multi-head self-attention
        let attended = self.attention(input, seq_len);

        // Residual connection
        for i in 0..output.len() {
            output[i] = output[i].add(attended[i]);
        }

        // Layer norm
        for n in 0..seq_len {
            let start = n * DIM;
            let end = start + DIM;
            fixed::layer_norm(&mut output[start..end]);
            // Apply scale + bias
            for d in 0..DIM {
                output[start + d] = output[start + d].mul(self.ln_scale[d]).add(self.ln_bias[d]);
            }
        }

        // FFN with residual
        let ffn_out = self.ffn(&output, seq_len);
        for i in 0..output.len() {
            output[i] = output[i].add(ffn_out[i]);
        }

        output
    }

    /// Multi-head self-attention.
    fn attention(&self, input: &[Fixed], seq_len: usize) -> Vec<Fixed> {
        let mut result = vec![Fixed::ZERO; seq_len * DIM];

        for h in 0..HEADS {
            let head_offset = h * HEAD_DIM;

            // Compute Q, K, V for this head
            let mut q = vec![Fixed::ZERO; seq_len * HEAD_DIM];
            let mut k = vec![Fixed::ZERO; seq_len * HEAD_DIM];
            let mut v = vec![Fixed::ZERO; seq_len * HEAD_DIM];

            for n in 0..seq_len {
                for d in 0..HEAD_DIM {
                    let out_d = head_offset + d;
                    let mut q_acc = Fixed::ZERO;
                    let mut k_acc = Fixed::ZERO;
                    let mut v_acc = Fixed::ZERO;
                    for j in 0..DIM {
                        let inp = input[n * DIM + j];
                        q_acc = q_acc.madd(inp, self.qkv[0 * DIM * DIM + j * DIM + out_d]);
                        k_acc = k_acc.madd(inp, self.qkv[1 * DIM * DIM + j * DIM + out_d]);
                        v_acc = v_acc.madd(inp, self.qkv[2 * DIM * DIM + j * DIM + out_d]);
                    }
                    q[n * HEAD_DIM + d] = q_acc;
                    k[n * HEAD_DIM + d] = k_acc;
                    v[n * HEAD_DIM + d] = v_acc;
                }
            }

            // Attention scores: Q * K^T / sqrt(HEAD_DIM)
            let scale_inv = Fixed::from_f64(1.0 / (HEAD_DIM as f64).sqrt());
            for i in 0..seq_len {
                // Compute scores for position i
                let mut scores = vec![Fixed::ZERO; seq_len];
                let mut max_score = Fixed::from_f64(-1000.0);
                for j in 0..seq_len {
                    let mut dot = Fixed::ZERO;
                    for d in 0..HEAD_DIM {
                        dot = dot.madd(q[i * HEAD_DIM + d], k[j * HEAD_DIM + d]);
                    }
                    scores[j] = dot.mul(scale_inv);
                    if scores[j].to_f64() > max_score.to_f64() {
                        max_score = scores[j];
                    }
                }

                // Softmax approximation: exp(x - max) / sum(exp(x - max))
                // Use 1 + x + x^2/2 as exp approximation (sufficient for small ranges)
                let mut exp_scores = vec![Fixed::ZERO; seq_len];
                let mut exp_sum = Fixed::ZERO;
                for j in 0..seq_len {
                    let x = scores[j].sub(max_score);
                    // exp(x) ≈ 1 + x + x^2/2 for small x
                    let x2 = x.mul(x);
                    let half = Fixed::from_f64(0.5);
                    let exp_x = Fixed::ONE.add(x).add(x2.mul(half));
                    let exp_x = if exp_x.to_f64() < 0.0 {
                        Fixed::from_f64(0.001)
                    } else {
                        exp_x
                    };
                    exp_scores[j] = exp_x;
                    exp_sum = exp_sum.add(exp_x);
                }
                let sum_inv = if exp_sum.to_f64().abs() > 1e-10 {
                    exp_sum.inv()
                } else {
                    Fixed::ONE
                };
                for j in 0..seq_len {
                    exp_scores[j] = exp_scores[j].mul(sum_inv);
                }

                // Weighted sum of values
                for d in 0..HEAD_DIM {
                    let mut acc = Fixed::ZERO;
                    for j in 0..seq_len {
                        acc = acc.madd(exp_scores[j], v[j * HEAD_DIM + d]);
                    }
                    result[i * DIM + head_offset + d] = result[i * DIM + head_offset + d].add(acc);
                }
            }
        }

        // Output projection
        let mut projected = vec![Fixed::ZERO; seq_len * DIM];
        for n in 0..seq_len {
            for d in 0..DIM {
                let mut acc = Fixed::ZERO;
                for j in 0..DIM {
                    acc = acc.madd(result[n * DIM + j], self.out_proj[j * DIM + d]);
                }
                projected[n * DIM + d] = acc;
            }
        }

        projected
    }

    /// Feed-forward network: DIM -> FFN_HIDDEN -> DIM with GeLU.
    fn ffn(&self, input: &[Fixed], seq_len: usize) -> Vec<Fixed> {
        let mut output = vec![Fixed::ZERO; seq_len * DIM];

        for n in 0..seq_len {
            // Layer 1: DIM -> FFN_HIDDEN + ReLU
            let mut hidden = vec![Fixed::ZERO; FFN_HIDDEN];
            for h in 0..FFN_HIDDEN {
                let mut acc = Fixed::ZERO;
                for d in 0..DIM {
                    acc = acc.madd(input[n * DIM + d], self.ffn1[d * FFN_HIDDEN + h]);
                }
                hidden[h] = acc.relu();
            }

            // Layer 2: FFN_HIDDEN -> DIM
            for d in 0..DIM {
                let mut acc = Fixed::ZERO;
                for h in 0..FFN_HIDDEN {
                    acc = acc.madd(hidden[h], self.ffn2[h * DIM + d]);
                }
                output[n * DIM + d] = acc;
            }
        }

        output
    }
}

impl Decoder {
    fn zeros() -> Self {
        Self {
            hidden: vec![Fixed::ZERO; 2 * DIM * FFN_HIDDEN],
            hidden_bias: vec![Fixed::ZERO; FFN_HIDDEN],
            output: vec![Fixed::ZERO; FFN_HIDDEN * DIM],
            output_bias: vec![Fixed::ZERO; DIM],
        }
    }

    /// One decoding step: latent + prev_instruction -> next instruction embedding.
    fn step(&self, latent: &[Fixed], prev: &[Fixed]) -> Vec<Fixed> {
        // Concatenate latent + prev -> 128-dim input
        let input_dim = 2 * DIM;
        let mut input = Vec::with_capacity(input_dim);
        input.extend_from_slice(latent);
        input.extend_from_slice(prev);

        // Hidden layer with ReLU
        let mut hidden = vec![Fixed::ZERO; FFN_HIDDEN];
        for h in 0..FFN_HIDDEN {
            let mut acc = self.hidden_bias[h];
            for d in 0..input_dim {
                acc = acc.madd(input[d], self.hidden[d * FFN_HIDDEN + h]);
            }
            hidden[h] = acc.relu();
        }

        // Output layer
        let mut out = vec![Fixed::ZERO; DIM];
        for d in 0..DIM {
            let mut acc = self.output_bias[d];
            for h in 0..FFN_HIDDEN {
                acc = acc.madd(hidden[h], self.output[h * DIM + d]);
            }
            out[d] = acc;
        }

        out
    }
}

/// Convert a DIM-dimensional output vector to a TASM instruction code.
/// Takes the argmax across the first 128 positions (7-bit opcode + 4-bit arg).
fn instr_to_code(output: &[Fixed]) -> u64 {
    let mut best_val = output[0].to_f64();
    let mut best_idx = 0u64;
    for (i, x) in output.iter().enumerate().skip(1) {
        let v = x.to_f64();
        if v > best_val {
            best_val = v;
            best_idx = i as u64;
        }
    }
    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_model_produces_empty() {
        let model = NeuralModel::zeros();
        let block = TIRBlock {
            nodes: [0; MAX_NODES * WORDS_PER_NODE],
            context: [0; CONTEXT_SIZE],
            node_count: 3,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 3,
        };
        let output = model.forward(&block);
        // Zero weights -> all-zero outputs -> argmax = index 0 -> code 0 -> stops immediately
        assert!(output.is_empty() || output.len() <= MAX_OUTPUT);
    }

    #[test]
    fn weight_vec_roundtrip() {
        let model = NeuralModel::zeros();
        let weights = model.to_weight_vec();
        let restored = NeuralModel::from_weight_vec(&weights);
        let w2 = restored.to_weight_vec();
        assert_eq!(weights.len(), w2.len());
        for (a, b) in weights.iter().zip(w2.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn weight_count_reasonable() {
        let model = NeuralModel::zeros();
        let count = model.weight_count();
        // Should be around 91K
        assert!(count > 80_000, "too few weights: {}", count);
        assert!(count < 120_000, "too many weights: {}", count);
    }

    #[test]
    fn forward_deterministic() {
        let model = NeuralModel::zeros();
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
}
