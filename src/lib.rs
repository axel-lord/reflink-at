#![doc = include_str!("../README.md")]

use std::{
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
    path::Path,
};

use nix::{fcntl::OFlag, unistd::UnlinkatFlags};

mod ioctl {
    #![allow(clippy::missing_docs_in_private_items)]
    use libc::FICLONE;
    use nix::ioctl_write_int_bad;

    ioctl_write_int_bad!(ficlone, FICLONE);
}

/// Re-export of Errno from nix crate.
pub use nix::errno::Errno;

/// Re-export of Mode from nix crate.
pub use nix::sys::stat::Mode;

/// How to handle existing files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OnExists {
    /// Only reflink by creating new files, error if destionation exists.
    CreateNewOnly,
    /// Only reflink by overriding files, error if destination does not exist.
    ExistsOnly,
    /// Create destination if it does not exists otherwise open it.
    Create,
}

impl OnExists {
    /// Get flags resulting in desired result.
    fn as_flags(self) -> OFlag {
        match self {
            OnExists::CreateNewOnly => OFlag::O_CREAT | OFlag::O_RDWR | OFlag::O_EXCL,
            OnExists::ExistsOnly => OFlag::O_RDWR,
            OnExists::Create => OFlag::O_CREAT | OFlag::O_RDWR,
        }
    }
}

/// Error type returned by [reflink_at] and [reflink_unlinked]
#[derive(Debug, ::thiserror::Error)]
pub enum ReflinkAtError {
    /// Unlink of file due to cleanup failed.
    #[error("cleanup due to '{src}' failed, {cleanup}")]
    CleanupUnlink {
        /// Reason for cleanup.
        src: Errno,
        /// Unlink error.
        cleanup: Errno,
    },

    /// Unlink of file failed.
    #[error("unlink failed, {0}")]
    Unlink(Errno),

    /// Forwarded [Errno].
    #[error(transparent)]
    Errno(#[from] Errno),
}

/// Attempt to create a reflink.
///
/// # Errors
/// If src does not support reading, or dest does not support writing, [Errno::EBADF] is returned.
/// EBADF might also be returned if the filesystem of src does not support reflinks.
///
/// [Errno::EXDEV] is returned if src and dest are not on the same filesystem.
///
/// If Either src or dest is not a regular file [Errno::EINVAL] might be returned.
pub fn reflink(dest: BorrowedFd, src: BorrowedFd) -> Result<(), Errno> {
    unsafe { ioctl::ficlone(dest.as_raw_fd(), src.as_raw_fd()) }?;
    Ok(())
}

/// Function to clean reflinks for [reflink_at].
fn cleanup(err: Errno, dirfd: Option<RawFd>, dest: &Path) -> ReflinkAtError {
    if let Err(cleanup) = nix::unistd::unlinkat(dirfd, dest, UnlinkatFlags::NoRemoveDir) {
        ReflinkAtError::CleanupUnlink { src: err, cleanup }
    } else {
        ReflinkAtError::Errno(err)
    }
}

/// Create a reflink with a path and dir fd as the destination. And return a file descriptor to the
/// created reflink.
///
/// # Errors
/// See [reflink], or if a file could not be created a dest.
pub fn reflink_at(
    dirfd: Option<BorrowedFd>,
    dest: &Path,
    src: BorrowedFd,
    mode: Mode,
    on_exists: OnExists,
) -> Result<OwnedFd, ReflinkAtError> {
    let dirfd = dirfd.map(|fd| fd.as_raw_fd());
    let dest_fd = nix::fcntl::openat(dirfd, dest, on_exists.as_flags(), mode)?;

    let dest_fd = unsafe { OwnedFd::from_raw_fd(dest_fd) };

    if let Err(err) = reflink(dest_fd.as_fd(), src) {
        return Err(cleanup(err, dirfd, dest));
    }

    Ok(dest_fd)
}

/// Create an unlinked reflink. Dest is required to know which filesystem to use.
///
/// # Errors
/// See [reflink_at], or if unlinking fails, on_exists i always [OnExists::CreateNewOnly].
pub fn reflink_unlinked(
    dirfd: Option<BorrowedFd>,
    dest: &Path,
    src: BorrowedFd,
    mode: Mode,
) -> Result<OwnedFd, Errno> {
    let fd = nix::fcntl::openat(
        dirfd.as_ref().map(|fd| fd.as_raw_fd()),
        dest,
        OFlag::O_RDWR | OFlag::O_TMPFILE,
        mode,
    )?;
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };

    reflink(fd.as_fd(), src)?;

    Ok(fd)
}
