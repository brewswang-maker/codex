//! Pending image attachments: files dropped onto the window or picked
//! through the native dialog, waiting to ride along with the next turn.
//!
//! The domain is tiny but has its own module so the drag-and-drop rules
//! (extension filter, dedup, hover tracking) stay testable headlessly.

use std::path::Path;
use std::path::PathBuf;

/// Image extensions the iced `image` widget decodes (the `image` crate's
/// common formats); anything else dropped on the window is ignored.
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff", "qoi", "avif",
];

/// Image attachments waiting to be submitted with the next turn.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Attachments {
    /// On-disk image paths, deduplicated in arrival order.
    pub images: Vec<PathBuf>,
    /// Whether a file drag is currently over the window.
    pub hovered: bool,
}

impl Attachments {
    /// Whether the file would render as an image attachment.
    pub fn is_image(path: &Path) -> bool {
        path.extension().is_some_and(|ext| {
            let ext = ext.to_string_lossy().to_lowercase();
            IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
    }

    /// Admits one dropped or picked file; returns whether it was accepted
    /// as a new image attachment.
    pub fn admit(&mut self, path: &Path) -> bool {
        if !Self::is_image(path) || self.images.iter().any(|known| known == path) {
            return false;
        }
        self.images.push(path.to_path_buf());
        true
    }

    /// Removes one pending attachment by position; returns whether the
    /// position existed.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.images.len() {
            return false;
        }
        self.images.remove(index);
        true
    }

    /// Clears the pending set after (or instead of) a submission.
    pub fn clear(&mut self) {
        self.images.clear();
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
