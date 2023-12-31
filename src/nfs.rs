use std::{cmp::min, mem::swap};

use log::{debug, info};
use rug::{
    integer::IntegerExt64,
    ops::{NegAssign, Pow},
    Complete, Integer,
};

use crate::{
    lanczos,
    linalg::CscMatrixBuilder,
    nt,
    params::{Params, OVERSQUARENESS},
    polynomial::{self, MpPolynomial, Polynomial},
    sqrt,
};

fn rational_factor_base(m: &Integer, params: &Params) -> Vec<(u64, u64)> {
    let mut base: Vec<(u64, u64)> = Vec::new();

    let mut p: u64 = 2;
    while base.len() < params.rational_base_size {
        if nt::miller_rabin(p) {
            base.push((p, m.mod_u64(p)));
        }
        p += 1;
    }

    base
}

fn algebraic_factor_base(f: &MpPolynomial, params: &Params) -> Vec<(u64, u64)> {
    let mut base: Vec<(u64, u64)> = Vec::new();
    let mut p: u64 = 2;

    while base.len() < params.algebraic_base_size {
        if nt::miller_rabin(p) {
            let roots = f.find_roots_mod_p(p);
            base.extend(roots.iter().map(|r| (p, *r)));
        }
        p += 1;
    }

    base.truncate(params.algebraic_base_size);
    base
}

fn quad_char_base(mut p: u64, f: &MpPolynomial, params: &Params) -> Vec<(u64, u64)> {
    let mut base: Vec<(u64, u64)> = Vec::new();
    let f_derivative = f.derivative();

    while base.len() < params.quad_char_base_size {
        if nt::miller_rabin(p) {
            let roots = f.find_roots_mod_p(p);
            for s in roots {
                if !f_derivative.evaluate(s).is_divisible_u64(p) {
                    base.push((p, s));
                }
            }
        }
        p += 1;
    }

    base.truncate(params.quad_char_base_size);
    base
}

fn ilog2_rounded(x: u64) -> u32 {
    ((x * x).ilog2() + 1) >> 1
}

fn line_sieve(b: u64, sieve_array: &mut Vec<i8>, base: &Vec<(u64, u64)>) {
    let a0 = -(sieve_array.len() as i64 / 2);

    for (p, r) in base {
        if b % p != 0 {
            let log2p = ilog2_rounded(*p) as i8;
            let mut i = (((-(((b * r) % p) as i64)) + *p as i64 - a0) % *p as i64) as usize;
            while i < sieve_array.len() {
                sieve_array[i] += log2p;
                i += *p as usize;
            }
        }
    }
}

fn norm(f: &MpPolynomial, a: i64, b: u64) -> Integer {
    let d = f.degree();
    let mut u = Integer::from(1);
    let mut v = Integer::from(-(b as i64)).pow(d as u32);
    let mut result = Integer::new();

    for coefficient in f.coefficients_ref().iter().take(d + 1) {
        result += coefficient * (&u * &v).complete();
        u *= a;
        v /= -(b as i64);
    }

    result
}

pub fn factorize(n: &Integer) -> Vec<Integer> {
    let params = Params::new(&n);
    let (f, m) = polynomial::select(n, &params);

    info!("set d = {}, m = {}", params.polynomial_degree, &m);
    info!("selected the polynomial {}", &f);

    // Maybe check that the polynomial is irreducible
    let rational_base = rational_factor_base(&m, &params);
    let algebraic_base = algebraic_factor_base(&f, &params);
    let quad_char_base = quad_char_base(algebraic_base.last().unwrap().0 + 1, &f, &params);

    let rational_begin: usize = 1;
    let algebraic_begin = rational_begin + rational_base.len();
    let quad_char_begin = algebraic_begin + algebraic_base.len();
    let base_len = quad_char_begin + quad_char_base.len();

    info!(
        "set up factor base consisting of {} primes on the rational side, {} prime ideals on the \
         algebraic side and {} quadratic characters (total size: {})",
        rational_base.len(),
        algebraic_base.len(),
        quad_char_base.len(),
        base_len
    );

    let mut matrix_builder = CscMatrixBuilder::new();
    matrix_builder.set_num_rows(quad_char_begin + quad_char_base.len());
    let mut relations: Vec<(i64, u64)> = Vec::new();

    let mut rational_sieve_array: Vec<i8> = vec![0; params.sieve_array_size];
    let mut algebraic_sieve_array: Vec<i8> = vec![0; params.sieve_array_size];

    for b in 1.. {
        rational_sieve_array
            .fill(-((ilog2_rounded(b) + m.significant_bits()) as i8) + params.rational_fudge);
        line_sieve(b, &mut rational_sieve_array, &rational_base);

        algebraic_sieve_array.fill(-params.algebraic_threshold);
        line_sieve(b, &mut algebraic_sieve_array, &algebraic_base);

        let a0 = -(params.sieve_array_size as i64 / 2);
        // Consider unsafe access here to avoid bounds checks.
        for i in 0..params.sieve_array_size {
            if rational_sieve_array[i] >= 0 && algebraic_sieve_array[i] >= 0 {
                let a = a0 + i as i64;
                if nt::gcd(((a % b as i64) + b as i64) as u64, b) != 1 || a == 0 {
                    continue;
                }

                let mut ones_pos: Vec<usize> = Vec::new();

                // Trial divide on the rational side.
                let mut num = a + (b * &m).complete();
                if num < 0 {
                    ones_pos.push(0);
                    num.neg_assign();
                }
                for (i, (p, _)) in rational_base.iter().enumerate() {
                    let e = num.remove_factor_mut(&Integer::from(*p));
                    if e & 1 == 1 {
                        ones_pos.push(rational_begin + i);
                    }
                }

                // Trial divide on the algebraic side.
                let mut alg_norm = norm(&f, a, b);
                for (i, (p, r)) in algebraic_base.iter().enumerate() {
                    if (a + b as i64 * *r as i64) % *p as i64 == 0 {
                        let e = alg_norm.remove_factor_mut(&Integer::from(*p));
                        if e & 1 == 1 {
                            ones_pos.push(algebraic_begin + i);
                        }
                    }
                }

                if num == 1 && alg_norm == 1 {
                    // smooth pair (a, b) found!
                    for (i, (p, s)) in quad_char_base.iter().enumerate() {
                        if nt::legendre(
                            (((a + b as i64 * *s as i64) % *p as i64 + *p as i64) % *p as i64)
                                as u64,
                            *p,
                        ) == p - 1
                        {
                            ones_pos.push(quad_char_begin + i);
                        }
                    }
                    matrix_builder.add_col(ones_pos);
                    relations.push((a, b));
                }
            }
        }

        if relations.len() >= base_len + OVERSQUARENESS {
            break;
        }
        debug!("collected {} relations", relations.len());
    }

    info!("collected {} relations", relations.len());

    let (mat, num_dependencies) = lanczos::find_dependencies(&matrix_builder.build());
    let mut factors: Vec<Integer> = Vec::new();

    for i in 0..num_dependencies {
        info!(
            "processing {}-{} dependency",
            i + 1,
            if i % 10 == 0 {
                "st"
            } else if i % 10 == 1 {
                "nd"
            } else if i % 10 == 2 {
                "rd"
            } else {
                "th"
            }
        );

        let mut rational: Vec<Integer> = Vec::new();
        let mut algebraic: Vec<MpPolynomial> = Vec::new();

        for (j, (a, b)) in relations.iter().enumerate() {
            if (mat[j] >> i) & 1 == 1 {
                rational.push(a + (b * &m).complete());
                let mut g = MpPolynomial::new();
                g[0] = Integer::from(*a);
                g[1] = Integer::from(*b);
                algebraic.push(g);
            }
        }

        let mut a = sqrt::mul_rational_integers(&rational).sqrt() * f.derivative().evaluate(&m);
        let mut b = match sqrt::algebraic_sqrt(
            &f.mul_mod(
                &sqrt::mul_algebraic_integers(&algebraic, &f),
                &f.mul_mod(&f.derivative(), &f.derivative()),
            ),
            &f,
        ) {
            Some(r) => r.evaluate(&m),
            None => continue,
        };
        assert_eq!(a.clone().square() % n, b.clone().square() % n);

        if a < b {
            swap(&mut a, &mut b);
        }

        {
            let x = (&a + &b).complete() % n;
            debug!("(a + b) % n = {}", x);
            let g = x.gcd(n);
            debug!("gcd(a + b, n) = {}", g);
            if g != 1 && &g != n {
                factors.push(min((n / &g).complete(), g));
            }
        }

        {
            let x = (&a - &b).complete() % n;
            debug!("(a - b) % n = {}", x);
            let g = x.gcd(n);
            debug!("gcd(a - b, n) = {}", g);
            if g != 1 && &g != n {
                factors.push(min((n / &g).complete(), g));
            }
        }
    }

    factors.sort_unstable();
    factors.dedup();

    factors
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMES_32: [u32; 3] = [100000007, 998244353, 1000000007];

    const PRIMES_64: [u64; 3] = [
        (1u64 << 60) + 33,
        (1u64 << 61) - 1,
        (1u64 << 61) + (1u64 << 56) + 61,
    ];

    // 2^3^5^7 + 2 - 3 - 5 - 7 is prime!
    const PRIMES_128: [u128; 1] = [2u128.pow(3).pow(5).pow(7) + 2 - 3 - 5 - 7];

    #[test]
    fn factorize_semiprime_64() {
        for i in 0..PRIMES_32.len() {
            for j in i + 1..PRIMES_32.len() {
                let factorization =
                    factorize(&(Integer::from(PRIMES_32[i]) * Integer::from(PRIMES_32[j])));
                assert_eq!(factorization.len(), 1);
                assert_eq!(factorization[0], PRIMES_32[i]);
            }
        }
    }

    #[ignore]
    #[test]
    fn factorize_semiprime_128() {
        for i in 0..PRIMES_64.len() {
            for j in i + 1..PRIMES_64.len() {
                let factorization =
                    factorize(&(Integer::from(PRIMES_64[i]) * Integer::from(PRIMES_64[j])));
                assert_eq!(factorization.len(), 1);
                assert_eq!(factorization[0], PRIMES_64[i]);
            }
        }
    }
}
