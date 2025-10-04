/// Performance benchmarks for KeystoneDB
///
/// Run with: cargo bench -p kstone-tests

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

fn bench_put_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("put_single");

    for size in [100, 1000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("size", size), &size, |b, &size| {
            let dir = TempDir::new().unwrap();
            let db = Database::create(dir.path()).unwrap();

            let value_str = "x".repeat(size);
            let item = ItemBuilder::new()
                .string("data", &value_str)
                .number("size", size as i64)
                .build();

            let mut counter = 0u64;
            b.iter(|| {
                let key = format!("key{}", counter);
                counter += 1;
                db.put(black_box(key.as_bytes()), black_box(item.clone()))
                    .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_put_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("put_batch");

    for batch_size in [100, 1000, 5000] {
        group.throughput(Throughput::Elements(batch_size));
        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let dir = TempDir::new().unwrap();
                    let db = Database::create(dir.path()).unwrap();

                    for i in 0..batch_size {
                        let key = format!("key{}", i);
                        let item = ItemBuilder::new()
                            .number("index", i as i64)
                            .string("data", format!("value{}", i))
                            .build();
                        db.put(black_box(key.as_bytes()), black_box(item)).unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_get_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_hit");

    for num_keys in [100, 1000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            // Setup: create database with known keys
            let dir = TempDir::new().unwrap();
            let db = Database::create(dir.path()).unwrap();

            for i in 0..num_keys {
                let key = format!("key{}", i);
                let item = ItemBuilder::new()
                    .number("index", i as i64)
                    .string("data", format!("value{}", i))
                    .build();
                db.put(key.as_bytes(), item).unwrap();
            }

            let mut counter = 0u64;
            b.iter(|| {
                let key = format!("key{}", counter % num_keys);
                counter += 1;
                let _result = db.get(black_box(key.as_bytes())).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_get_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_miss");

    for num_keys in [100, 1000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            // Setup: create database with known keys
            let dir = TempDir::new().unwrap();
            let db = Database::create(dir.path()).unwrap();

            for i in 0..num_keys {
                let key = format!("key{}", i);
                let item = ItemBuilder::new().number("index", i as i64).build();
                db.put(key.as_bytes(), item).unwrap();
            }

            let mut counter = 0u64;
            b.iter(|| {
                // Query non-existent keys
                let key = format!("missing{}", counter);
                counter += 1;
                let _result = db.get(black_box(key.as_bytes())).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("delete");

    for num_keys in [100, 1000, 5000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            b.iter(|| {
                // Setup fresh database for each iteration
                let dir = TempDir::new().unwrap();
                let db = Database::create(dir.path()).unwrap();

                for i in 0..num_keys {
                    let key = format!("key{}", i);
                    let item = ItemBuilder::new().number("value", i as i64).build();
                    db.put(key.as_bytes(), item).unwrap();
                }

                // Benchmark: delete one key
                db.delete(black_box(b"key0")).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("flush");

    for num_keys in [500, 1000, 2000] {
        group.throughput(Throughput::Elements(num_keys));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            b.iter(|| {
                let dir = TempDir::new().unwrap();
                let db = Database::create(dir.path()).unwrap();

                // Write records
                for i in 0..num_keys {
                    let key = format!("key{}", i);
                    let item = ItemBuilder::new()
                        .number("index", i as i64)
                        .string("data", format!("value{}", i))
                        .build();
                    db.put(key.as_bytes(), item).unwrap();
                }

                // Benchmark: flush to SST
                db.flush().unwrap();
            });
        });
    }
    group.finish();
}

fn bench_recovery(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery");

    for num_keys in [100, 500, 1000] {
        group.throughput(Throughput::Elements(num_keys));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            // Setup: create database with data, then close
            let dir = TempDir::new().unwrap();
            let path = dir.path().to_path_buf();

            {
                let db = Database::create(&path).unwrap();
                for i in 0..num_keys {
                    let key = format!("key{}", i);
                    let item = ItemBuilder::new()
                        .number("index", i as i64)
                        .string("data", format!("value{}", i))
                        .build();
                    db.put(key.as_bytes(), item).unwrap();
                }
                // Close database (WAL has unflushed data)
            }

            // Benchmark: recovery time (opening and replaying WAL)
            b.iter(|| {
                let _db = Database::open(black_box(&path)).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");

    for ops in [100, 500, 1000] {
        group.throughput(Throughput::Elements(ops));
        group.bench_with_input(BenchmarkId::new("ops", ops), &ops, |b, &ops| {
            b.iter(|| {
                let dir = TempDir::new().unwrap();
                let db = Database::create(dir.path()).unwrap();

                // Mixed workload: 50% writes, 30% reads, 20% deletes
                for i in 0..ops {
                    match i % 10 {
                        0..=4 => {
                            // Write (50%)
                            let key = format!("key{}", i);
                            let item = ItemBuilder::new()
                                .number("value", i as i64)
                                .build();
                            db.put(key.as_bytes(), item).unwrap();
                        }
                        5..=7 => {
                            // Read (30%)
                            let key = format!("key{}", i / 2);
                            let _result = db.get(key.as_bytes()).unwrap();
                        }
                        _ => {
                            // Delete (20%)
                            let key = format!("key{}", i / 3);
                            db.delete(key.as_bytes()).unwrap();
                        }
                    }
                }
            });
        });
    }
    group.finish();
}

fn bench_composite_keys(c: &mut Criterion) {
    let mut group = c.benchmark_group("composite_keys");

    for num_pks in [10, 50, 100] {
        let num_sks = 20;
        let total_ops = num_pks * num_sks;
        group.throughput(Throughput::Elements(total_ops as u64));

        group.bench_with_input(BenchmarkId::new("pks", num_pks), &num_pks, |b, &num_pks| {
            b.iter(|| {
                let dir = TempDir::new().unwrap();
                let db = Database::create(dir.path()).unwrap();

                for pk_id in 0..num_pks {
                    for sk_id in 0..num_sks {
                        let pk = format!("partition{}", pk_id);
                        let sk = format!("sort{}", sk_id);
                        let item = ItemBuilder::new()
                            .number("pk_id", pk_id as i64)
                            .number("sk_id", sk_id as i64)
                            .build();
                        db.put_with_sk(
                            black_box(pk.as_bytes()),
                            black_box(sk.as_bytes()),
                            black_box(item),
                        )
                        .unwrap();
                    }
                }
            });
        });
    }
    group.finish();
}

fn bench_in_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("in_memory");

    for num_keys in [100, 1000, 5000] {
        group.throughput(Throughput::Elements(num_keys));
        group.bench_with_input(BenchmarkId::new("keys", num_keys), &num_keys, |b, &num_keys| {
            b.iter(|| {
                let db = Database::create_in_memory().unwrap();

                for i in 0..num_keys {
                    let key = format!("key{}", i);
                    let item = ItemBuilder::new()
                        .number("index", i as i64)
                        .string("data", format!("value{}", i))
                        .build();
                    db.put(black_box(key.as_bytes()), black_box(item)).unwrap();
                }

                // Read back
                for i in 0..num_keys {
                    let key = format!("key{}", i);
                    let _result = db.get(black_box(key.as_bytes())).unwrap();
                }
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_put_single,
    bench_put_batch,
    bench_get_hit,
    bench_get_miss,
    bench_delete,
    bench_flush,
    bench_recovery,
    bench_mixed_workload,
    bench_composite_keys,
    bench_in_memory
);
criterion_main!(benches);
