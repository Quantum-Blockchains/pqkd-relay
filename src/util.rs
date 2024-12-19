pub fn xor(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let c = a.iter().zip(b.iter()).map(|(&x1, &x2)| x1 ^ x2).collect();
    c
}
