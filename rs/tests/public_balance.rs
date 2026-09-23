use bbg::proof::{open_cell, prove_balances, prove_commitment, prove_public_balance};
use bbg::query_auth::verify_public_balance;
use bbg::{BbgState, Dim, balance_key};

#[test]
fn full_u64_balance_requires_exact_root_key_and_both_limbs() {
    let (owner, token) = ([7; 32], [8; 32]);
    let mut state = BbgState::new();
    state.balances.insert(balance_key(&owner, &token), u64::MAX);
    state.commitments.insert([9; 32], nebu::Goldilocks::new(42));
    state.refresh_root();
    let root = state.root();
    let proof = prove_public_balance(&state, &owner, &token).unwrap();
    assert_eq!(
        verify_public_balance(&proof, &root, &owner, &token),
        Some(u64::MAX)
    );
    assert_eq!(
        verify_public_balance(&proof, &[0; 32], &owner, &token),
        None
    );
    assert_eq!(verify_public_balance(&proof, &root, &[6; 32], &token), None);
    assert_eq!(verify_public_balance(&proof, &root, &owner, &[6; 32]), None);
    let mut altered = proof.clone();
    let context = altered.context.as_mut().unwrap();
    context.certificate.dimensions[0].fields[context.index as usize + 1] -= 1;
    assert_eq!(verify_public_balance(&altered, &root, &owner, &token), None);
    let context = proof.context.as_ref().unwrap();
    assert_eq!(context.certificate.dimensions.len(), 1);
    assert_eq!(context.certificate.dimensions[0].namespace, 10);
    for legacy in [
        prove_balances(&state, &owner, &token).unwrap(),
        open_cell(&state, Dim::Balances, 11).unwrap(),
        prove_commitment(&state, &[9; 32]).unwrap(),
    ] {
        assert!(legacy.context.is_none());
        assert_eq!(verify_public_balance(&legacy, &root, &owner, &token), None);
    }
    state.balances.insert(balance_key(&owner, &token), 1);
    assert!(
        prove_public_balance(&state, &owner, &token).is_none(),
        "stale cache"
    );
    state.refresh_root();
    assert_eq!(
        verify_public_balance(&proof, &state.root(), &owner, &token),
        None
    );
}
