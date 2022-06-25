//! Utilities for calculating perceptual hashes of images.

pub use image;
pub use img_hash;

/// Get the hasher used by default in all FuzzySearch related projects.
pub fn get_hasher() -> img_hash::Hasher<[u8; 8]> {
    use img_hash::{HashAlg::Gradient, HasherConfig};

    HasherConfig::with_bytes_type::<[u8; 8]>()
        .hash_alg(Gradient)
        .hash_size(8, 8)
        .preproc_dct()
        .to_hasher()
}
