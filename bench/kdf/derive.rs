//! Compiled by run.mjs with the workspace's release rlibs, outside the Cargo test graph.
use pinveil_core::protocol::{KDF_ITERATIONS_MAX, KDF_ITERATIONS_MIN};
use sha2::Sha256;
use std::hint::black_box;
use std::time::Instant;
use zeroize::Zeroizing;

// The runner extracts the unchanged private function from production crypto.rs.
include!(env!("PINVEIL_KDF_SOURCE"));

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .expect("run count")
        .parse()
        .expect("integer runs");
    assert!(runs >= 5 && runs % 2 == 1);
    let salt_for = |sample: u32| {
        let mut salt: [u8; 16] = std::array::from_fn(|i| i as u8);
        salt[12..].copy_from_slice(&sample.to_be_bytes());
        salt
    };
    for iterations in [KDF_ITERATIONS_MIN, KDF_ITERATIONS_MAX] {
        let derive = |salt: &[u8]| {
            derive_kek(
                black_box("alpha-bravo-charlie-delta"),
                black_box("123456"),
                black_box(salt),
                black_box(iterations),
            )
        };
        black_box(derive(&salt_for(0)));
        let mut samples = Vec::with_capacity(runs);
        let mut hex = String::new();
        for i in 1..=runs {
            let salt = salt_for(i as u32);
            let start = Instant::now();
            let key = black_box(derive(&salt));
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            hex = key.iter().map(|byte| format!("{byte:02x}")).collect();
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let salt_hex: String = salt_for(runs as u32)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        println!(
            "{{\"iterations\":{iterations},\"samplesMs\":{samples:?},\"medianMs\":{},\"keyHex\":\"{hex}\",\"saltHex\":\"{salt_hex}\"}}",
            sorted[runs / 2],
        );
    }
}
