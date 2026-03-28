// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use std::path::{Component, Path, PathBuf};

/// Resolve a user-provided path and verify it stays within the jail root.
/// For paths that exist on disk, uses canonicalize for symlink resolution.
/// For non-existent paths, validates all existing prefix components to prevent
/// symlink-based escape.
///
/// Rejects absolute paths — use [`safe_resolve_within`] for paths that may
/// legitimately be absolute (e.g. expression-evaluated working directories).
pub fn safe_resolve(base: &Path, user_path: &str) -> Result<PathBuf, Error> {
    let path = Path::new(user_path);

    // Reject absolute paths — they bypass the base entirely
    if path.is_absolute() {
        return Err(Error::PathTraversal(user_path.to_string()));
    }

    resolve_under(base, &base.join(user_path), user_path)
}

/// Like [`safe_resolve`], but allows absolute paths as long as they resolve
/// under one of the given `allowed_roots`.
///
/// This is needed for composite action working directories where expression
/// evaluation (e.g. `$GITHUB_ACTION_PATH/subdir`) produces absolute paths
/// that point into the actions cache — a legitimate location.
pub fn safe_resolve_within(
    base: &Path,
    user_path: &str,
    allowed_roots: &[&Path],
) -> Result<PathBuf, Error> {
    let path = Path::new(user_path);

    if path.is_absolute() {
        // Absolute path: must be under one of the allowed roots
        let canonical = if path.exists() {
            path.canonicalize()
                .map_err(|e| Error::PathTraversal(format!("cannot resolve '{}': {e}", user_path)))?
        } else {
            // Validate existing prefix components
            validate_prefix_components(path, allowed_roots, user_path)?;
            path.to_path_buf()
        };
        for root in allowed_roots {
            if let Ok(root_canonical) = root.canonicalize()
                && canonical.starts_with(&root_canonical)
            {
                return Ok(canonical);
            }
        }
        return Err(Error::PathTraversal(format!(
            "'{}' is not under any allowed root",
            user_path
        )));
    }

    // Relative path: resolve under base
    resolve_under(base, &base.join(user_path), user_path)
}

/// Core validation: verify `joined` stays within `base` (handles symlinks, `..`).
fn resolve_under(base: &Path, joined: &Path, user_path: &str) -> Result<PathBuf, Error> {
    // Reject any `..` components
    for component in Path::new(user_path).components() {
        if component == Component::ParentDir {
            return Err(Error::PathTraversal(user_path.to_string()));
        }
    }

    let base_canonical = if base.exists() {
        base.canonicalize().map_err(|e| {
            Error::PathTraversal(format!("cannot resolve base '{}': {e}", base.display()))
        })?
    } else {
        base.to_path_buf()
    };

    // If the path exists, canonicalize to resolve symlinks
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|e| Error::PathTraversal(format!("cannot resolve '{}': {e}", user_path)))?;
        if !canonical.starts_with(&base_canonical) {
            return Err(Error::PathTraversal(format!(
                "'{}' resolves outside of '{}'",
                user_path,
                base.display()
            )));
        }
        Ok(canonical)
    } else {
        // Path doesn't exist yet — validate each existing prefix component
        // to ensure no symlink along the way escapes the jail.
        let mut current = base_canonical.clone();
        for component in Path::new(user_path).components() {
            if let Component::Normal(seg) = component {
                current.push(seg);
                if current.exists() {
                    let resolved = current.canonicalize().map_err(|e| {
                        Error::PathTraversal(format!("cannot resolve '{}': {e}", current.display()))
                    })?;
                    if !resolved.starts_with(&base_canonical) {
                        return Err(Error::PathTraversal(format!(
                            "'{}' resolves outside of '{}'",
                            user_path,
                            base.display()
                        )));
                    }
                    current = resolved;
                }
            }
        }
        Ok(joined.to_path_buf())
    }
}

/// For an absolute non-existent path, walk its components and check that
/// every existing prefix is under one of the allowed roots.
fn validate_prefix_components(
    path: &Path,
    allowed_roots: &[&Path],
    user_path: &str,
) -> Result<(), Error> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if current.exists()
            && current.components().count() > 1
            && let Ok(resolved) = current.canonicalize()
        {
            let under_any = allowed_roots.iter().any(|root| {
                root.canonicalize()
                    .map(|r| resolved.starts_with(&r))
                    .unwrap_or(false)
            });
            if !under_any {
                return Err(Error::PathTraversal(format!(
                    "'{}' is not under any allowed root",
                    user_path
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_path() {
        let base = Path::new("/tmp");
        let result = safe_resolve(base, "foo/bar.txt").unwrap();
        assert_eq!(result, PathBuf::from("/tmp/foo/bar.txt"));
    }

    #[test]
    fn test_reject_parent_dir() {
        let base = Path::new("/tmp");
        let err = safe_resolve(base, "../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn test_reject_hidden_traversal() {
        let base = Path::new("/tmp");
        let err = safe_resolve(base, "foo/../../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn test_reject_absolute_path() {
        let base = Path::new("/tmp");
        let err = safe_resolve(base, "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn test_existing_path() {
        // /tmp exists, so canonicalize should work
        let base = Path::new("/tmp");
        let result = safe_resolve(base, "nonexistent_safe_file.txt").unwrap();
        assert!(result.to_string_lossy().contains("nonexistent_safe_file"));
    }

    #[test]
    fn test_symlink_detection() {
        // Create a symlink that escapes the jail
        let jail = tempfile::tempdir().unwrap();
        let escape_target = tempfile::tempdir().unwrap();
        let link_path = jail.path().join("escape");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(escape_target.path(), &link_path).unwrap();
            // The symlink "escape" points outside the jail, so "escape/secret.txt"
            // must be rejected even though the full path doesn't exist on disk.
            let result = safe_resolve(jail.path(), "escape/secret.txt");
            assert!(result.is_err(), "symlink-based escape should be rejected");
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(escape_target.path(), &link_path).unwrap();
            let result = safe_resolve(jail.path(), "escape/secret.txt");
            assert!(result.is_err(), "symlink-based escape should be rejected");
        }
    }

    #[test]
    fn test_safe_resolve_within_allows_absolute_under_root() {
        let root = tempfile::tempdir().unwrap();
        let subdir = root.path().join("actions");
        std::fs::create_dir_all(&subdir).unwrap();
        let abs_path = subdir.to_string_lossy().to_string();
        let result = safe_resolve_within(root.path(), &abs_path, &[root.path()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safe_resolve_within_rejects_absolute_outside_roots() {
        let root = tempfile::tempdir().unwrap();
        let result = safe_resolve_within(root.path(), "/etc/passwd", &[root.path()]);
        assert!(result.is_err());
    }
}
