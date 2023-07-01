use rand::{thread_rng, Rng, RngCore};

// TODO: make this module generic
//       If this module becomes a bottleneck, use something like LKK / Montgomery

const fn mod_exp(mut a: u64, mut b: u64, n: u64) -> u64 {
    let mut c: u64 = 1;

    while b != 0 {
        if b & 1 == 1 {
            c = (c * a) % n;
        }
        a = (a * a) % n;
        b >>= 1;
    }

    c
}

const fn legendre(a: u64, p: u64) -> u64 {
    mod_exp(a, (p - 1) >> 1, p)
}

const fn mod_inverse(a: u64, n: u64) -> u64 {
    mod_exp(a, n - 2, n)
}

// Finds a square root of a modulo p using the Tonelli-Shanks algorithm. a and p may not be greater
// than u32::MAX, since multiplication is performed with them.
pub fn mod_sqrt(mut a: u64, p: u64) -> u64 {
    assert!(a <= u32::MAX as u64);
    assert!(p <= u32::MAX as u64);
    if p == 2 {
        assert_eq!(a, 1);
        return 1;
    }

    assert_eq!(legendre(a, p), 1);

    if p & 3 == 3 {
        return mod_exp(a, (p + 1) >> 2, p);
    }

    let mut rng = thread_rng();

    // About 2 iterations are expected.
    let mut b = rng.gen_range(2..p - 1);
    while legendre(b, p) == 1 {
        b = rng.gen_range(2..p - 1);
    }

    // Loop invariant: c = b ^ (2 ^ (k - 2)). Before the loop, k = 2, which is possible since p = 1
    // mod 4, so m = (p - 1) / 2^k is an integer.
    let mut m = (p - 1) >> 2;
    let mut correction: u64 = 1;
    let mut c = b;
    let mut cinv = mod_inverse(b, p);

    loop {
        if mod_exp(a, m, p) != 1 {
            a = (a * ((c * c) % p)) % p;
            correction = (correction * cinv) % p;
        }
        if m & 1 == 1 {
            break;
        }
        m >>= 1;
        c = (c * c) % p;
        cinv = (cinv * cinv) % p;
    }

    (mod_exp(a, (m + 1) >> 1, p) * correction) % p
}

// TODO: Add Cipolla's algorithm (it shall be faster sometimes?)

#[cfg(test)]
mod tests {
    use super::*;

    // Returns true, if (and only if? I'm not sure.) n is a prime. Works for numbers less than
    // u32::MAX.
    fn is_prime(n: u64) -> bool {
        assert!(n <= u32::MAX as u64);

        const MILLER_RABIN_BASES: [u64; 3] = [15, 7363882082, 992620450144556];

        let trailing_zeros = (n - 1).trailing_zeros();
        let u = (n - 1) >> trailing_zeros;

        for mut a in MILLER_RABIN_BASES {
            a = a % n;
            let mut x = mod_exp(a, u, n);
            for _ in 0..trailing_zeros {
                let y = (x * x) % n;
                if y == 1 && x != 1 && x != n - 1 {
                    return false;
                }
                x = y;
            }
            if x != 1 {
                return false;
            }
        }
        true
    }

    // TODO: find better algorithm
    fn gen_prime() -> u32 {
        let mut rng = thread_rng();
        loop {
            let p = rng.next_u32();
            if is_prime(p as u64) {
                return p;
            }
        }
    }

    #[test]
    fn test_tonelli_shanks() {
        let mut rng = thread_rng();
        for _ in 0..10000 {
            let p = gen_prime();
            let mut a = rng.gen_range(2..p - 1);
            while legendre(a as u64, p as u64) != 1 {
                a = rng.gen_range(2..p - 1);
            }
            let x = mod_sqrt(a as u64, p as u64);
            assert_eq!((x * x) % p as u64, a as u64);
        }
    }
}
