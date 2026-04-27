use std::path::{Component, Path, PathBuf};

pub fn normalize_absolute(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_parent_segments() {
        let path = Path::new("/usr/lib/../lib64/./libc.so.6");
        assert_eq!(
            normalize_absolute(path),
            Some(PathBuf::from("/usr/lib64/libc.so.6"))
        );
    }
}
