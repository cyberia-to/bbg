use super::*;
use crate::proof::QueryProof;
use crate::storage::mem::MemStore;
use nebu::Goldilocks;

fn g(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}
fn key(b: u8) -> [u8; 32] {
    [b; 32]
}

struct MockNetworkStore(Vec<u8>);

impl NetworkStore for MockNetworkStore {
    fn fetch(&self, _particle: &Particle) -> Option<Vec<u8>> {
        Some(self.0.clone())
    }
    fn das_sample(&self, _particle: &Particle, _offset: u64, _length: u64) -> Option<QueryProof> {
        None
    }
}

#[test]
fn fetch_content_accepts_matching_content() {
    let content = b"hello world".to_vec();
    let particle = *hemera::hash(&content).as_bytes();
    let store = TieredStore::new(Box::new(MemStore::new()))
        .with_network(Box::new(MockNetworkStore(content.clone())));
    assert_eq!(store.fetch_content(&particle), Some(content));
}

#[test]
fn fetch_content_rejects_content_that_does_not_hash_to_the_particle() {
    let particle = *hemera::hash(b"hello world").as_bytes();
    let store = TieredStore::new(Box::new(MemStore::new()))
        .with_network(Box::new(MockNetworkStore(b"hello world!".to_vec())));
    assert_eq!(store.fetch_content(&particle), None);
}

#[test]
fn fetch_content_none_without_a_network_tier() {
    let store = TieredStore::new(Box::new(MemStore::new()));
    assert_eq!(store.fetch_content(&[0; 32]), None);
}

#[test]
fn write_through_to_warm() {
    let hot = Box::new(MemStore::new());
    let warm = Box::new(MemStore::new());
    let warm_ptr = &*warm as *const MemStore as usize;
    let mut store = TieredStore::new(hot).with_warm(warm).unwrap();

    store.put(0, key(1), vec![g(42)]).unwrap();

    // Value readable from hot
    assert_eq!(store.hot.get(0, &key(1)), Some([g(42)].as_slice()));
    // Value also in warm (write-through)
    assert_eq!(
        store.warm.as_ref().unwrap().get(0, &key(1)),
        Some([g(42)].as_slice())
    );
    let _ = warm_ptr; // suppress warning
}

#[test]
fn read_cascades_hot_then_warm() {
    let hot = Box::new(MemStore::new());
    let mut warm = Box::new(MemStore::new());
    warm.put(0, key(2), vec![g(99)]).unwrap();
    // hot does NOT have key(2)
    let _ = hot.get(0, &key(2));

    let store = TieredStore::new(hot).with_warm(warm).unwrap();
    assert_eq!(store.get(0, &key(2)), Some([g(99)].as_slice()));
}

#[test]
fn hot_hit_shadows_warm() {
    let mut hot = Box::new(MemStore::new());
    let mut warm = Box::new(MemStore::new());
    hot.put(0, key(3), vec![g(1)]).unwrap();
    warm.put(0, key(3), vec![g(2)]).unwrap(); // different value in warm

    let store = TieredStore::new(hot).with_warm(warm).unwrap();
    // HOT wins
    assert_eq!(store.get(0, &key(3)), Some([g(1)].as_slice()));
}

#[test]
fn promote_moves_warm_to_hot() {
    let hot = Box::new(MemStore::new());
    let mut warm = Box::new(MemStore::new());
    warm.put(0, key(4), vec![g(77)]).unwrap();

    let mut store = TieredStore::new(hot).with_warm(warm).unwrap();
    assert!(store.hot.get(0, &key(4)).is_none());

    let promoted = store.promote(0, &key(4)).unwrap();
    assert!(promoted);
    assert_eq!(store.hot.get(0, &key(4)), Some([g(77)].as_slice()));
}

#[test]
fn ephemeral_not_written_to_warm() {
    let hot = Box::new(MemStore::new());
    let warm = Box::new(MemStore::new());
    let mut store = TieredStore::new(hot).with_warm(warm).unwrap();

    store.put(dim::EPHEMERAL, key(5), vec![g(123)]).unwrap();

    assert_eq!(
        store.hot.get(dim::EPHEMERAL, &key(5)),
        Some([g(123)].as_slice())
    );
    assert!(
        store
            .warm
            .as_ref()
            .unwrap()
            .get(dim::EPHEMERAL, &key(5))
            .is_none(),
        "EPHEMERAL must not be written to warm tier"
    );
}

#[test]
fn ephemeral_not_in_dirty_after_commit() {
    let mut store = TieredStore::default();
    store.put(dim::EPHEMERAL, key(6), vec![g(7)]).unwrap();
    assert!(
        store.dirty_entries().is_empty(),
        "EPHEMERAL must not appear in dirty"
    );
}

#[test]
fn remove_clears_from_all_tiers() {
    let hot = Box::new(MemStore::new());
    let warm = Box::new(MemStore::new());
    let mut store = TieredStore::new(hot).with_warm(warm).unwrap();

    store.put(0, key(7), vec![g(55)]).unwrap();
    let removed = store.remove(0, &key(7)).unwrap();

    assert_eq!(removed, Some(vec![g(55)]));
    assert!(store.hot.get(0, &key(7)).is_none());
    assert!(store.warm.as_ref().unwrap().get(0, &key(7)).is_none());
}

#[test]
fn get_mut_and_mark_dirty_roundtrip() {
    let mut store = TieredStore::default();
    store.put(0, key(8), vec![g(10), g(20)]).unwrap();
    store.commit().unwrap(); // clear dirty

    {
        let slice = store.get_mut(0, &key(8)).unwrap();
        slice[0] = g(99);
    }
    store.mark_dirty(0, key(8)).unwrap();

    let dirty = store.dirty_entries();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].2[0], g(99));
}

#[test]
fn iter_returns_dimension_entries() {
    let mut store = TieredStore::default();
    store.put(0, key(1), vec![g(1)]).unwrap();
    store.put(0, key(2), vec![g(2)]).unwrap();
    store.put(1, key(3), vec![g(3)]).unwrap();

    let dim0: Vec<_> = store.iter(0).collect();
    assert_eq!(dim0.len(), 2);
    let dim1: Vec<_> = store.iter(1).collect();
    assert_eq!(dim1.len(), 1);
}
