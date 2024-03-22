//! Utilities for calculating perceptual hashes of images.

use std::ops::Deref;

pub use image;

/// A tool for creating perceptual image hashes.
pub struct ImageHasher(image_hasher::Hasher<[u8; 8]>);

/// A perceptual image hash.
pub struct ImageHash(pub [u8; 8]);

impl Default for ImageHasher {
    fn default() -> Self {
        use image_hasher::{HashAlg::Gradient, HasherConfig};

        let hasher = HasherConfig::with_bytes_type::<[u8; 8]>()
            .hash_alg(Gradient)
            .hash_size(8, 8)
            .preproc_dct()
            .to_hasher();

        Self(hasher)
    }
}

impl ImageHasher {
    /// Hash an image.
    pub fn hash_image<I>(&self, im: &I) -> ImageHash
    where
        I: image_hasher::Image,
    {
        let hash = self.0.hash_image(im);
        ImageHash(hash.into_inner())
    }
}

impl Deref for ImageHash {
    type Target = [u8; 8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<ImageHash> for i64 {
    fn from(val: ImageHash) -> i64 {
        i64::from_be_bytes(val.0)
    }
}

impl From<i64> for ImageHash {
    fn from(value: i64) -> Self {
        Self(value.to_be_bytes())
    }
}
