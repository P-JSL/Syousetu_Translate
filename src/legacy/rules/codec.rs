//! The observed complement transform is separate from the container framing.
//! It never guesses an encoding or drops bytes from a record.
pub fn complement(encoded: &[u8]) -> Vec<u8> {
    encoded.iter().map(|byte| !byte).collect()
}
