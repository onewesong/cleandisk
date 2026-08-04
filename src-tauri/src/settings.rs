use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub project_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub project_roots: Vec<String>,
}

const DEFAULT_ROOTS: &[&str] = &["Code", "Developer", "Projects", "workspace", "work"];
const DENIED: &[&str] = &[
    "Library",
    ".Trash",
    "CloudStorage",
    "Mobile Documents",
    "Applications",
    ".ssh",
    ".gnupg",
];

pub fn defaults(home: &Path) -> Settings {
    Settings {
        project_roots: DEFAULT_ROOTS
            .iter()
            .map(|name| home.join(name))
            .filter(|p| p.is_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    }
}

pub fn validate_roots(home: &Path, roots: Vec<String>) -> Result<Settings, String> {
    let home = home.canonicalize().map_err(|e| e.to_string())?;
    let home_dev = fs::metadata(&home).map_err(|e| e.to_string())?.dev();
    let mut valid = Vec::new();
    for raw in roots {
        let requested = PathBuf::from(&raw);
        let symlink_meta = fs::symlink_metadata(&requested).map_err(|e| format!("{raw}: {e}"))?;
        if symlink_meta.file_type().is_symlink() {
            return Err(format!("不允许符号链接根目录：{raw}"));
        }
        let path = requested
            .canonicalize()
            .map_err(|e| format!("{raw}: {e}"))?;
        if path == home || !path.starts_with(&home) {
            return Err(format!("项目目录必须位于用户目录内：{raw}"));
        }
        let relative = path.strip_prefix(&home).map_err(|e| e.to_string())?;
        if relative
            .components()
            .any(|c| DENIED.iter().any(|d| c.as_os_str() == *d))
        {
            return Err(format!("不允许扫描敏感目录：{raw}"));
        }
        if fs::metadata(&path).map_err(|e| e.to_string())?.dev() != home_dev {
            return Err(format!("项目目录必须与用户目录位于同一文件系统：{raw}"));
        }
        let normalized = path.to_string_lossy().into_owned();
        if !valid.contains(&normalized) {
            valid.push(normalized);
        }
    }
    valid.sort();
    Ok(Settings {
        project_roots: valid,
    })
}

pub fn load(path: &Path, home: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .and_then(|s| validate_roots(home, s.project_roots).ok())
        .unwrap_or_else(|| defaults(home))
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(&data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;
    #[test]
    fn validates_deduplicates_and_sorts_roots() {
        let home = tempdir().unwrap();
        let a = home.path().join("A");
        let b = home.path().join("B");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let result = validate_roots(
            home.path(),
            vec![
                b.display().to_string(),
                a.display().to_string(),
                a.display().to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            result.project_roots,
            vec![
                a.canonicalize().unwrap().display().to_string(),
                b.canonicalize().unwrap().display().to_string()
            ]
        );
    }
    #[test]
    fn rejects_symlink_and_sensitive_roots() {
        let home = tempdir().unwrap();
        let code = home.path().join("Code");
        fs::create_dir(&code).unwrap();
        let linked = home.path().join("linked");
        symlink(&code, &linked).unwrap();
        assert!(
            validate_roots(home.path(), vec![linked.display().to_string()])
                .unwrap_err()
                .contains("符号链接")
        );
        let sensitive = home.path().join("Library/project");
        fs::create_dir_all(&sensitive).unwrap();
        assert!(
            validate_roots(home.path(), vec![sensitive.display().to_string()])
                .unwrap_err()
                .contains("敏感")
        );
    }
    #[test]
    fn atomic_save_round_trips() {
        let home = tempdir().unwrap();
        let root = home.path().join("Code");
        fs::create_dir(&root).unwrap();
        let path = home.path().join("config/settings.json");
        let settings = validate_roots(home.path(), vec![root.display().to_string()]).unwrap();
        save(&path, &settings).unwrap();
        assert_eq!(load(&path, home.path()), settings);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
