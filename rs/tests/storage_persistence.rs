use bbg::storage::{MemStore, ShardStore, TieredStore, dim};
use nebu::Goldilocks;

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
struct StorePath(std::path::PathBuf);

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
impl StorePath {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-storage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
impl Drop for StorePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
fn update_hot_value(store: &mut TieredStore) {
    store
        .put(dim::PARTICLES, [1; 32], vec![Goldilocks::new(10)])
        .unwrap();
    store.commit().unwrap();
    store.get_mut(dim::PARTICLES, &[1; 32]).unwrap()[0] = Goldilocks::new(99);
    store.mark_dirty(dim::PARTICLES, [1; 32]).unwrap();
    store.commit().unwrap();
}

#[test]
fn eviction_keeps_ephemeral_in_hot() {
    let mut store = TieredStore::new(Box::new(MemStore::new()))
        .with_warm(Box::new(MemStore::new()))
        .unwrap();
    store
        .put(dim::EPHEMERAL, [1; 32], vec![Goldilocks::new(99)])
        .unwrap();
    store.evict(dim::EPHEMERAL, &[1; 32]).unwrap();
    assert_eq!(
        store.get_mut(dim::EPHEMERAL, &[1; 32]),
        Some([Goldilocks::new(99)].as_mut_slice())
    );
}

#[test]
fn eviction_without_warm_preserves_the_last_copy() {
    let mut store = TieredStore::default();
    store
        .put(dim::PARTICLES, [1; 32], vec![Goldilocks::new(99)])
        .unwrap();
    store.commit().unwrap();
    store.evict(dim::PARTICLES, &[1; 32]).unwrap();
    assert_eq!(
        store.get(dim::PARTICLES, &[1; 32]),
        Some([Goldilocks::new(99)].as_slice())
    );
}

#[cfg(feature = "backend-ssd")]
#[test]
fn fjall_commit_and_hot_mutation_survive_reopen() {
    use bbg::storage::FjallStore;
    let path = StorePath::new();
    {
        let warm = FjallStore::open(path.0.join("warm")).unwrap();
        let mut store = TieredStore::new(Box::new(MemStore::new()))
            .with_warm(Box::new(warm))
            .unwrap();
        update_hot_value(&mut store);
    }
    let reopened = FjallStore::open(path.0.join("warm")).unwrap();
    assert_eq!(
        reopened.load(dim::PARTICLES, &[1; 32]).unwrap(),
        Some(vec![Goldilocks::new(99)])
    );
}

#[cfg(feature = "backend-hdd")]
#[test]
fn redb_commit_and_hot_mutation_survive_reopen() {
    use bbg::storage::RedbStore;
    let path = StorePath::new();
    {
        let warm = RedbStore::open(path.0.join("warm.redb")).unwrap();
        let mut store = TieredStore::new(Box::new(MemStore::new()))
            .with_warm(Box::new(warm))
            .unwrap();
        update_hot_value(&mut store);
    }
    let reopened = RedbStore::open(path.0.join("warm.redb")).unwrap();
    assert_eq!(
        reopened.load(dim::PARTICLES, &[1; 32]).unwrap(),
        Some(vec![Goldilocks::new(99)])
    );
}

#[cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]
#[test]
fn disk_backends_persist_dimensions_overwrites_and_keep_ephemeral_local() {
    use bbg::storage::{FjallStore, RedbStore};
    let path = StorePath::new();
    {
        let mut stores: Vec<Box<dyn ShardStore>> = vec![
            Box::new(FjallStore::open(path.0.join("ssd")).unwrap()),
            Box::new(RedbStore::open(path.0.join("hdd.redb")).unwrap()),
        ];
        for store in &mut stores {
            for d in 0..=dim::EPHEMERAL {
                store.put(d, [d; 32], vec![Goldilocks::new(10)]).unwrap();
                store.commit().unwrap();
                store.put(d, [d; 32], vec![Goldilocks::new(99)]).unwrap();
            }
            store.commit().unwrap();
        }
    }
    let ssd = FjallStore::open(path.0.join("ssd")).unwrap();
    let hdd = RedbStore::open(path.0.join("hdd.redb")).unwrap();
    for d in 0..dim::EPHEMERAL {
        assert_eq!(
            ssd.load(d, &[d; 32]).unwrap(),
            Some(vec![Goldilocks::new(99)])
        );
        assert_eq!(
            hdd.load(d, &[d; 32]).unwrap(),
            Some(vec![Goldilocks::new(99)])
        );
    }
    assert_eq!(
        ssd.load(dim::EPHEMERAL, &[dim::EPHEMERAL; 32]).unwrap(),
        None
    );
    assert_eq!(
        hdd.load(dim::EPHEMERAL, &[dim::EPHEMERAL; 32]).unwrap(),
        None
    );
    // These observations expose the current borrowed-cache API boundary.
    // Disk load succeeds above; shared get/iter still cannot discover cold data.
    println!(
        "fjall reopen: disk_load=99, cache_get={:?}, cache_iter={}",
        ssd.get(dim::PARTICLES, &[0; 32]),
        ssd.iter(dim::PARTICLES).count()
    );
    println!(
        "redb reopen: disk_load=99, cache_get={:?}, cache_iter={}",
        hdd.get(dim::PARTICLES, &[0; 32]),
        hdd.iter(dim::PARTICLES).count()
    );
}
