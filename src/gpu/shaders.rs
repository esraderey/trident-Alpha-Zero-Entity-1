/// Goldilocks field arithmetic (canonical form).
#[allow(dead_code)]
pub const GOLDILOCKS: &str = include_str!("shaders/goldilocks.wgsl");

/// Fixed-point arithmetic over Goldilocks (scale = 2^16).
#[allow(dead_code)]
pub const FIXED_POINT: &str = include_str!("shaders/fixed_point.wgsl");

/// Neural forward pass (float32, MLP-only, batched dispatch).
pub const NEURAL_FORWARD: &str = include_str!("shaders/neural_forward.wgsl");
