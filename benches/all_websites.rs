use criterion::{
    criterion_group,
    criterion_main,
    measurement::{Measurement, ValueFormatter},
    Criterion,
    Throughput,
};
use mach_6;
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy)]
struct TscCycles;

struct CycleFormatter;

static CYCLE_FORMATTER: CycleFormatter = CycleFormatter;

impl ValueFormatter for CycleFormatter {
    fn scale_values(&self, typical_value: f64, values: &mut [f64]) -> &'static str {
        let (factor, unit) = if typical_value < 1_000.0 {
            (1.0, "cycles")
        } else if typical_value < 1_000_000.0 {
            (1e-3, "Kcycles")
        } else if typical_value < 1_000_000_000.0 {
            (1e-6, "Mcycles")
        } else {
            (1e-9, "Gcycles")
        };
        values.iter_mut().for_each(|value| *value *= factor);
        unit
    }

    fn scale_throughputs(
        &self,
        _typical_value: f64,
        throughput: &Throughput,
        values: &mut [f64],
    ) -> &'static str {
        let work = match *throughput {
            Throughput::Bytes(value)
            | Throughput::BytesDecimal(value)
            | Throughput::Bits(value)
            | Throughput::Elements(value) => value as f64,
        };
        values.iter_mut().for_each(|value| *value = work / *value);
        "operations/cycle"
    }

    fn scale_for_machines(&self, _values: &mut [f64]) -> &'static str {
        "cycles"
    }
}

impl Measurement for TscCycles {
    type Intermediate = tsc_timer::Start;
    type Value = tsc_timer::Duration;

    fn start(&self) -> Self::Intermediate {
        tsc_timer::Start::now()
    }

    fn end(&self, start: Self::Intermediate) -> Self::Value {
        start.elapsed()
    }

    fn add(&self, left: &Self::Value, right: &Self::Value) -> Self::Value {
        *left + *right
    }

    fn zero(&self) -> Self::Value {
        tsc_timer::Duration::default()
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        value.cycles() as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &CYCLE_FORMATTER
    }
}

fn bench_all_websites(c: &mut Criterion<TscCycles>) {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let websites = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let documents_selectors = match mach_6::get_documents_and_selectors(&websites) {
        Ok(documents_selectors) => documents_selectors,
        Err(e) => return eprintln!("ERROR: {e}"),
    };
    for res in documents_selectors {
        match res {
            Ok((name, document, selectors)) => {
                let selector_map = mach_6::build_selector_map(&selectors);
                let mut group = c.benchmark_group(&name);
                group.bench_function("Naive", |b| b.iter(|| {
                    mach_6::match_selectors(&document, &selectors);
                }));
                group.bench_function("With SelectorMap", |b| b.iter(|| {
                    mach_6::match_selectors_with_selector_map(&document, &selector_map);
                }));
                group.bench_function("With SelectorMap and Bloom Filter", |b| b.iter(|| {
                    mach_6::match_selectors_with_bloom_filter(&document, &selector_map);
                }));
                group.bench_function("With SelectorMap, Bloom Filter, and Style Sharing", |b| b.iter(|| {
                    mach_6::match_selectors_with_style_sharing(&document, &selector_map);
                }));
            },
            Err(e) => {
                eprintln!("ERROR: {e}");
            }
        }
    }
}

fn cycle_criterion() -> Criterion<TscCycles> {
    Criterion::default()
        .with_measurement(TscCycles)
        .warm_up_time(Duration::from_millis(500))
        .sample_size(25)
}

criterion_group! {
    name = benches;
    config = cycle_criterion();
    targets = bench_all_websites
}
criterion_main!(benches);
