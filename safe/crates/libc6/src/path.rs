use core_runtime::path::normalize_absolute;
use std::path::{Path, PathBuf};

pub fn normalized_realpath_input(path: &Path) -> Option<PathBuf> {
    normalize_absolute(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_paths() {
        assert_eq!(normalized_realpath_input(Path::new("tmp/file")), None);
    }
}
