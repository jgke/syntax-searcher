use criterion::{black_box, criterion_group, criterion_main, Criterion};

use syns::tokenizer::*;
use syns::options::Options;
use std::fs::File;

fn bench_tokenizer_dict(c: &mut Criterion) {
    let options = Options::new("txt".as_ref(), &["syns", "foo", "-"]);
    let filename = "/usr/share/dict/words";
    let mut group = c.benchmark_group("tokenizer dict");
    group.bench_function("tokenizer dict", |b| {
        b.iter(|| {
            let content = File::open(&filename).unwrap();
            tokenize(filename, content, &options)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_tokenizer_dict);
criterion_main!(benches);
