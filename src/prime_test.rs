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

// Returns true, if (and only if? I'm not sure.) n is a prime.
pub fn miller_rabin(n: u32) -> bool {
    if n == 2 {
        return true;
    }

    const MILLER_RABIN_BASES: [u64; 3] = [15, 7363882082, 992620450144556];

    let trailing_zeros = (n - 1).trailing_zeros();
    let u = (n - 1) >> trailing_zeros;

    for mut a in MILLER_RABIN_BASES {
        a = a % n as u64;
        if a == 0 {
            continue;
        }

        let mut x = mod_exp(a, u as u64, n as u64);
        for _ in 0..trailing_zeros {
            let y = (x * x) % n as u64;
            if y == 1 && x != 1 && x != n as u64 - 1 {
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
