//! Utilities for calculating perceptual hashes of images.

use std::ops::Deref;

pub use image;

/// A tool for creating perceptual image hashes.
pub struct ImageHasher(img_hash::Hasher<[u8; 8]>);

/// A perceptual image hash.
pub struct ImageHash(pub [u8; 8]);

impl Default for ImageHasher {
    fn default() -> Self {
        Self(get_hasher())
    }
}

impl ImageHasher {
    /// Hash an image.
    pub fn hash_image<I>(&self, im: &I) -> ImageHash
    where
        I: img_hash::Image,
    {
        let hash = self.0.hash_image(im);
        let bytes: [u8; 8] = hash.as_bytes().try_into().unwrap();

        ImageHash(bytes)
    }
}

impl Deref for ImageHash {
    type Target = [u8; 8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Into<i64> for ImageHash {
    fn into(self) -> i64 {
        i64::from_be_bytes(self.0)
    }
}

impl From<i64> for ImageHash {
    fn from(value: i64) -> Self {
        Self(value.to_be_bytes())
    }
}

/// Get the hasher used by default in all FuzzySearch related projects.
fn get_hasher() -> img_hash::Hasher<[u8; 8]> {
    use img_hash::{HashAlg::Gradient, HasherConfig};

    HasherConfig::with_bytes_type::<[u8; 8]>()
        .hash_alg(Gradient)
        .hash_size(8, 8)
        .preproc_dct()
        .to_hasher()
}
