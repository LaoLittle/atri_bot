use atri_core::{decode, encode};

#[test]
fn integer_contract_preserves_values_outside_javascript_precision() {
    // Foreign-language callers must not lose identifiers through a JSON/f64 bridge.
    let wire = [0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
    assert_eq!(decode::<u64>(&wire).unwrap(), 9_007_199_254_740_993);
    assert_eq!(
        decode::<u64>(&encode(&u64::MAX).unwrap()).unwrap(),
        u64::MAX
    );
    assert_eq!(
        decode::<i64>(&encode(&i64::MIN).unwrap()).unwrap(),
        i64::MIN
    );
    assert!(decode::<i64>(&encode(&u64::MAX).unwrap()).is_err());
    assert!(decode::<u64>(&encode(&-1i64).unwrap()).is_err());
}

#[test]
fn decoder_rejects_truncated_and_concatenated_frames() {
    let valid = encode(&"extension-call").unwrap();
    let mut concatenated = valid.clone();
    concatenated.extend_from_slice(&encode(&()).unwrap());
    assert_eq!(
        decode::<String>(&concatenated).unwrap_err().code,
        "invalid_payload"
    );
    assert_eq!(
        decode::<String>(&valid[..valid.len() - 1])
            .unwrap_err()
            .code,
        "invalid_payload"
    );
    assert_eq!(
        decode::<String>(&[0xff]).unwrap_err().code,
        "invalid_payload"
    );
}
