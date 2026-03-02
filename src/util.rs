pub fn xor(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let c = a.iter().zip(b.iter()).map(|(&x1, &x2)| x1 ^ x2).collect();
    c
}

#[cfg(test)]
mod tests {
    use super::xor;

    #[test]
    fn xor_roundtrip_with_same_mask_recovers_original_data() {
        let a = b"relay-key".to_vec();
        let b = b"mask-1234".to_vec();

        let encrypted = xor(a.clone(), b.clone());
        let decrypted = xor(encrypted, b);

        assert_eq!(decrypted, a);
    }

    #[test]
    fn xor_uses_shorter_input_length() {
        let out = xor(vec![1, 2, 3, 4], vec![9, 8]);
        assert_eq!(out, vec![8, 10]);
    }
}
