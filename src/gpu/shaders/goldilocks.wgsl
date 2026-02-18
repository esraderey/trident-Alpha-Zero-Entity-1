// Canonical Goldilocks field arithmetic for GPU compute.
//
// Field: p = 2^64 - 2^32 + 1 = 0xFFFFFFFF00000001
// Elements stored as vec2<u32>(lo, hi) since WGSL lacks native u64.
// Canonical form (NOT Montgomery): element = raw u64 mod p.
//
// Reusable library — no neural-specific logic.

const GL_P_LO: u32 = 0x00000001u;
const GL_P_HI: u32 = 0xFFFFFFFFu;

fn gl_add(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let lo = a.x + b.x;
    let carry_lo = select(0u, 1u, lo < a.x);
    let hi = a.y + b.y + carry_lo;
    let carry_hi = select(0u, 1u, hi < a.y || (carry_lo == 1u && hi == a.y));
    var r = vec2<u32>(lo, hi);
    if carry_hi == 1u || hi > GL_P_HI || (hi == GL_P_HI && lo >= GL_P_LO) {
        let sub_lo = r.x - GL_P_LO;
        let borrow = select(0u, 1u, r.x < GL_P_LO);
        let sub_hi = r.y - GL_P_HI - borrow;
        r = vec2<u32>(sub_lo, sub_hi);
    }
    return r;
}

fn gl_sub(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    if a.y > b.y || (a.y == b.y && a.x >= b.x) {
        let lo = a.x - b.x;
        let borrow = select(0u, 1u, a.x < b.x);
        let hi = a.y - b.y - borrow;
        return vec2<u32>(lo, hi);
    }
    let ap_lo = a.x + GL_P_LO;
    let carry = select(0u, 1u, ap_lo < a.x);
    let ap_hi = a.y + GL_P_HI + carry;
    let lo = ap_lo - b.x;
    let borrow = select(0u, 1u, ap_lo < b.x);
    let hi = ap_hi - b.y - borrow;
    return vec2<u32>(lo, hi);
}

fn mul32(a: u32, b: u32) -> vec2<u32> {
    let a_lo = a & 0xFFFFu;
    let a_hi = a >> 16u;
    let b_lo = b & 0xFFFFu;
    let b_hi = b >> 16u;
    let p0 = a_lo * b_lo;
    let p1 = a_lo * b_hi;
    let p2 = a_hi * b_lo;
    let p3 = a_hi * b_hi;
    let mid = p1 + (p0 >> 16u);
    let mid2 = (mid & 0xFFFFu) + p2;
    let lo = ((mid2 & 0xFFFFu) << 16u) | (p0 & 0xFFFFu);
    let hi = p3 + (mid >> 16u) + (mid2 >> 16u);
    return vec2<u32>(lo, hi);
}

fn gl_reduce(v: vec2<u32>) -> vec2<u32> {
    if v.y > GL_P_HI || (v.y == GL_P_HI && v.x >= GL_P_LO) {
        let lo = v.x - GL_P_LO;
        let borrow = select(0u, 1u, v.x < GL_P_LO);
        let hi = v.y - GL_P_HI - borrow;
        return vec2<u32>(lo, hi);
    }
    return v;
}

fn canon_reduce128(lo: vec2<u32>, hi: vec2<u32>) -> vec2<u32> {
    let m0 = mul32(hi.x, 0xFFFFFFFFu);
    let m1 = mul32(hi.y, 0xFFFFFFFFu);
    var r0 = m0.x;
    var r1 = m0.y;
    var r2 = 0u;
    let t1 = r1 + m1.x;
    let c1 = select(0u, 1u, t1 < r1);
    r1 = t1;
    r2 = m1.y + c1;
    let hs_lo = vec2<u32>(r0, r1);
    let hs_hi = vec2<u32>(r2, 0u);
    let sum_lo_x = lo.x + hs_lo.x;
    let c2 = select(0u, 1u, sum_lo_x < lo.x);
    let sum_lo_y = lo.y + hs_lo.y + c2;
    let c3 = select(0u, 1u, sum_lo_y < lo.y || (c2 == 1u && sum_lo_y == lo.y));
    let sum_hi_x = hs_hi.x + c3;
    let c4 = select(0u, 1u, sum_hi_x < hs_hi.x);
    let sum_hi_y = hs_hi.y + c4;
    let s_lo = vec2<u32>(sum_lo_x, sum_lo_y);
    let s_hi = vec2<u32>(sum_hi_x, sum_hi_y);
    if s_hi.x == 0u && s_hi.y == 0u {
        return gl_reduce(s_lo);
    }
    let m2 = mul32(s_hi.x, 0xFFFFFFFFu);
    let m3 = mul32(s_hi.y, 0xFFFFFFFFu);
    var q0 = m2.x;
    var q1 = m2.y;
    var q2 = 0u;
    let t2 = q1 + m3.x;
    let c5 = select(0u, 1u, t2 < q1);
    q1 = t2;
    q2 = m3.y + c5;
    let ss_lo_x = s_lo.x + q0;
    let c6 = select(0u, 1u, ss_lo_x < s_lo.x);
    let ss_lo_y = s_lo.y + q1 + c6;
    let c7 = select(0u, 1u, ss_lo_y < s_lo.y || (c6 == 1u && ss_lo_y == s_lo.y));
    let rem_hi = q2 + c7;
    var result = vec2<u32>(ss_lo_x, ss_lo_y);
    if rem_hi > 0u {
        let corr = rem_hi * 0xFFFFFFFFu;
        let rc_lo = result.x + corr;
        let rc_carry = select(0u, 1u, rc_lo < result.x);
        let rc_hi = result.y + rc_carry;
        result = vec2<u32>(rc_lo, rc_hi);
    }
    return gl_reduce(result);
}

fn canon_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let ll = mul32(a.x, b.x);
    let lh = mul32(a.x, b.y);
    let hl = mul32(a.y, b.x);
    let hh = mul32(a.y, b.y);
    var r0 = ll.x;
    var r1 = ll.y;
    var r2 = hh.x;
    var r3 = hh.y;
    let t1 = r1 + lh.x;
    let c1 = select(0u, 1u, t1 < r1);
    r1 = t1;
    let t2 = r2 + lh.y + c1;
    let c2 = select(0u, 1u, t2 < r2 || (c1 == 1u && t2 == r2));
    r2 = t2;
    r3 = r3 + c2;
    let t3 = r1 + hl.x;
    let c3 = select(0u, 1u, t3 < r1);
    r1 = t3;
    let t4 = r2 + hl.y + c3;
    let c4 = select(0u, 1u, t4 < r2 || (c3 == 1u && t4 == r2));
    r2 = t4;
    r3 = r3 + c4;
    return canon_reduce128(vec2<u32>(r0, r1), vec2<u32>(r2, r3));
}

fn gl_field_inv(a: vec2<u32>) -> vec2<u32> {
    var result = vec2<u32>(1u, 0u);
    var base = a;
    for (var i = 0u; i < 32u; i++) {
        result = canon_mul(result, base);
        base = canon_mul(base, base);
    }
    base = canon_mul(base, base);
    for (var i = 1u; i < 32u; i++) {
        result = canon_mul(result, base);
        base = canon_mul(base, base);
    }
    return result;
}
