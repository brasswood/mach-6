use criterion::{criterion_group, criterion_main, Criterion};
use mach_6;
pub fn bench_all_websites(c: &mut Criterion) {
    
}

criterion_group!(benches, bench_all_websites);
criterion_main!(benches);