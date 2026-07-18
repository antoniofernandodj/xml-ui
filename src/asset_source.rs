//! Asset resolution seam — the layer between a logical asset *path* and its
//! *bytes*.
//!
//! Every template (`.gv`/`.kdl`), stylesheet (`.gss`), theme/data JSON, Luau
//! script and binary asset (SVG/image) the engine loads at runtime goes through
//! an [`AssetSource`] instead of touching `std::fs` directly. The default
//! [`DiskAssets`] reads from the filesystem exactly as the engine always did,
//! so nothing changes for existing consumers.
//!
//! The point of the seam is to let a consumer swap in an *embedded* source: a
//! standalone release binary can bundle all its assets at compile time (e.g.
//! via `include_dir!`) behind an `AssetSource` and run with the working
//! directory pointing anywhere — the binary becomes fully decoupled from the
//! asset tree on disk. In that mode [`AssetSource::modified`] returns `None`,
//! which naturally disables hot-reload (there is no file to watch).
//!
//! Paths handed to an `AssetSource` are the same logical strings the engine
//! resolves today (CWD-relative, e.g. `crates/app/views/app.gv`, or a Luau
//! module path joined against its caller). A disk source resolves them against
//! the filesystem; an embedded source normalizes and looks them up in its
//! bundle. See [`GlacierDaemon::assets`](crate::GlacierDaemon::assets).

use std::borrow::Cow;
use std::time::SystemTime;

/// Resolves logical asset paths to their contents.
///
/// Implementations must be cheap to share (`Arc<dyn AssetSource>`), thread-safe
/// (`Send + Sync`), and free of interior mutability surprises — the engine may
/// call these from render and from background reload checks.
pub trait AssetSource: Send + Sync + std::fmt::Debug {
    /// Reads a binary asset (image/SVG). `Cow` so an embedded source can hand
    /// back a borrow of its `'static` bundle without copying.
    fn read_bytes(&self, path: &str) -> std::io::Result<Cow<'static, [u8]>>;

    /// Reads a text asset (template/stylesheet/JSON/Luau) as UTF-8.
    fn read_to_string(&self, path: &str) -> std::io::Result<Cow<'static, str>>;

    /// Whether an asset exists at `path`. Used by the Luau module resolver to
    /// probe candidate module files without reading them.
    fn exists(&self, path: &str) -> bool;

    /// Last-modified time, or `None` when the source cannot change under the
    /// running process (an embedded bundle). Hot-reload keys off this: a `None`
    /// means "never reloads", so [`GlacierUI::check_reload`](crate::GlacierUI::check_reload)
    /// becomes a no-op for embedded assets.
    fn modified(&self, path: &str) -> Option<SystemTime>;
}

/// The default [`AssetSource`]: reads straight from the filesystem, preserving
/// the engine's original behavior (CWD-relative paths, live hot-reload).
#[derive(Debug, Default, Clone, Copy)]
pub struct DiskAssets;

impl AssetSource for DiskAssets {
    fn read_bytes(&self, path: &str) -> std::io::Result<Cow<'static, [u8]>> {
        std::fs::read(path).map(Cow::Owned)
    }

    fn read_to_string(&self, path: &str) -> std::io::Result<Cow<'static, str>> {
        std::fs::read_to_string(path).map(Cow::Owned)
    }

    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).is_file()
    }

    fn modified(&self, path: &str) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }
}
