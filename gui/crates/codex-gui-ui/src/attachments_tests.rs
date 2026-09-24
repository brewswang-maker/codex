//! Drag-and-drop admission rules for [`Attachments`].

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn admits_dropped_images_in_arrival_order() {
    let mut attachments = Attachments::default();

    assert!(attachments.admit(std::path::Path::new("/tmp/shot.png")));
    assert!(attachments.admit(std::path::Path::new("/tmp/diagram.PNG")));
    assert_eq!(
        attachments.images,
        vec![
            std::path::PathBuf::from("/tmp/shot.png"),
            std::path::PathBuf::from("/tmp/diagram.PNG"),
        ],
    );
}

#[test]
fn rejects_non_image_files_and_directories() {
    let mut attachments = Attachments::default();

    assert!(!attachments.admit(std::path::Path::new("/tmp/notes.txt")));
    assert!(!attachments.admit(std::path::Path::new("/tmp/archive.tar.gz")));
    assert!(!attachments.admit(std::path::Path::new("/tmp")));
    assert!(attachments.images.is_empty());
}

#[test]
fn deduplicates_repeated_drops_of_the_same_file() {
    let mut attachments = Attachments::default();

    assert!(attachments.admit(std::path::Path::new("/tmp/shot.png")));
    assert!(!attachments.admit(std::path::Path::new("/tmp/shot.png")));
    assert_eq!(attachments.images.len(), 1);
}

#[test]
fn remove_drops_only_the_targeted_position() {
    let mut attachments = Attachments::default();
    let _ = attachments.admit(std::path::Path::new("/tmp/a.png"));
    let _ = attachments.admit(std::path::Path::new("/tmp/b.png"));
    let _ = attachments.admit(std::path::Path::new("/tmp/c.png"));

    assert!(attachments.remove(1));
    assert!(!attachments.remove(2));
    assert_eq!(
        attachments.images,
        vec![
            std::path::PathBuf::from("/tmp/a.png"),
            std::path::PathBuf::from("/tmp/c.png"),
        ],
    );
}

#[test]
fn clear_empties_the_pending_set() {
    let mut attachments = Attachments::default();
    let _ = attachments.admit(std::path::Path::new("/tmp/a.png"));

    attachments.clear();

    assert!(attachments.images.is_empty());
}
