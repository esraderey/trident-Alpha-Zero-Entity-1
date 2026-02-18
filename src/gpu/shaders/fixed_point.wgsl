// Fixed-point arithmetic over canonical Goldilocks.
//
// Scale factor S = 2^16 = 65536. Positive reals encoded as raw field elements,
// negatives as p - |v|*S. Requires goldilocks.wgsl to be prepended.
//
// inv_scale and half_p are passed via uniform params (computed on CPU).

const FP_ZERO: vec2<u32> = vec2<u32>(0u, 0u);
const FP_ONE_LO: u32 = 65536u;
const FP_ONE_HI: u32 = 0u;

fn fp_one() -> vec2<u32> { return vec2<u32>(FP_ONE_LO, FP_ONE_HI); }

fn inv_scale() -> vec2<u32> {
    return vec2<u32>(params.inv_scale_lo, params.inv_scale_hi);
}

fn half_p() -> vec2<u32> {
    return vec2<u32>(params.half_p_lo, params.half_p_hi);
}

fn fp_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return canon_mul(canon_mul(a, b), inv_scale());
}

fn fp_relu(x: vec2<u32>) -> vec2<u32> {
    let hp = half_p();
    if x.y > hp.y || (x.y == hp.y && x.x > hp.x) { return FP_ZERO; }
    return x;
}

fn fp_gt(a: vec2<u32>, b: vec2<u32>) -> bool {
    let hp = half_p();
    let a_neg = a.y > hp.y || (a.y == hp.y && a.x > hp.x);
    let b_neg = b.y > hp.y || (b.y == hp.y && b.x > hp.x);
    if !a_neg && b_neg { return true; }
    if a_neg && !b_neg { return false; }
    return a.y > b.y || (a.y == b.y && a.x > b.x);
}

fn fp_inv(x: vec2<u32>) -> vec2<u32> {
    if x.x == 0u && x.y == 0u { return fp_one(); }
    let x_inv = gl_field_inv(x);
    let scale = vec2<u32>(FP_ONE_LO, FP_ONE_HI);
    let scale_sq = canon_mul(scale, scale);
    return canon_mul(scale_sq, x_inv);
}

fn fp_inv_u32(n: u32) -> vec2<u32> {
    let val = 65536u / n;
    return vec2<u32>(val, 0u);
}
