use criterion::{criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use mach_6;

pub fn bench_all_websites(c: &mut Criterion) {
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let documents_selectors = match mach_6::get_documents_and_selectors(&websites) {
        Ok(documents_selectors) => documents_selectors,
        Err(e) => return eprintln!("ERROR: {e}"),
    };
    for res in documents_selectors {
        match res {
            Ok((name, document, selectors)) => {
                c.bench_function(&name, |b| b.iter(|| {
                    mach_6::match_selectors(&document, &selectors);
                }));
            },
            Err(e) => {
                eprintln!("ERROR: {e}");
            }
        }
    }
}

criterion_group!(benches, bench_all_websites);
criterion_main!(benches);