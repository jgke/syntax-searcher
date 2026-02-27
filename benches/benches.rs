use criterion::{criterion_group, criterion_main, Criterion};

use std::fs;
use syns::options::Options;
use syns::tokenizer::*;

fn bench_tokenizer_dict(c: &mut Criterion) {
    let options = Options::new("txt".as_ref(), &["syns", "foo", "-"]);
    let filename = "/usr/share/dict/words";
    let content = fs::read_to_string(filename).unwrap_or_default();
    let mut group = c.benchmark_group("tokenizer dict");
    group.bench_function("tokenizer dict", |b| {
        b.iter(|| tokenize(&content, &options))
    });
    group.finish();
}

criterion_group!(benches, bench_tokenizer_dict);
criterion_main!(benches);
