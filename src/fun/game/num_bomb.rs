use rand::rngs::ThreadRng;
use rand::{thread_rng, Rng, RngCore};

pub struct NumBomb {
    num: u16,
    start: u16,
    end: u16,
}

impl NumBomb {
    pub fn new() -> Self {
        let mut rng = thread_rng();
        let start = rng.gen_range(0..u16::MAX);
        let end = rng.gen_range(0..u16::MAX);
        let num = rng.gen_range(start..=end);

        Self { num, start, end }
    }
}
