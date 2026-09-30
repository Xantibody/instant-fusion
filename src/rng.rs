//! SplitMix64. A seed has to give the same wallpaper forever, so the
//! generator lives here, where no dependency update can change its output.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1)
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [a, b)
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }

    /// Uniform in 0..n
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// True with probability `p`
    pub fn coin(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_and_below_stay_inside_their_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let x = r.range(-2.0, 3.0);
            assert!((-2.0..3.0).contains(&x));
            assert!(r.below(5) < 5);
        }
    }

    #[test]
    fn a_coin_lands_true_about_as_often_as_asked() {
        let mut r = Rng::new(7);
        let hits = (0..10_000).filter(|_| r.coin(0.3)).count();
        assert!((2_800..3_200).contains(&hits), "{hits}");
    }
}
