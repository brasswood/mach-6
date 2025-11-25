use criterion::{criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use mach_6;

pub fn bench_all_websites(c: &mut Criterion) {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let documents_selectors = match mach_6::get_documents_and_selectors(&websites) {
        Ok(documents_selectors) => documents_selectors,
        Err(e) => return eprintln!("ERROR: {e}"),
    };
    for res in documents_selectors {
        match res {
            Ok((name, document, selectors)) => {
                let elements = mach_6::get_elements(&document);
                let selector_map = mach_6::build_selector_map(&selectors);
                let mut group = c.benchmark_group(&name);
                group.bench_function("Naive", |b| b.iter(|| {
                    mach_6::match_selectors(&elements, &selectors);
                }));
                group.bench_function("With SelectorMap", |b| b.iter(|| {
                    mach_6::match_selectors_with_selector_map(&elements, &selector_map);
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