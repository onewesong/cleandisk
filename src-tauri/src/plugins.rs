use crate::model::{Category, Risk};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::WalkDir;

pub struct ScanContext {
    pub home: PathBuf,
    pub now: i64,
    pub project_roots: Vec<PathBuf>,
}
pub struct RawCandidate {
    pub plugin: String,
    pub category: Category,
    pub risk: Risk,
    pub path: PathBuf,
    pub reason: String,
}
pub trait CleanerPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn discover(
        &self,
        context: &ScanContext,
        cancel: &AtomicBool,
    ) -> Result<Vec<RawCandidate>, String>;
}

fn raw(
    plugin: &str,
    category: Category,
    risk: Risk,
    path: PathBuf,
    reason: impl Into<String>,
) -> RawCandidate {
    RawCandidate {
        plugin: plugin.into(),
        category,
        risk,
        path,
        reason: reason.into(),
    }
}

fn children(base: &Path) -> Vec<PathBuf> {
    fs::read_dir(base)
        .map(|it| {
            it.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| !p.is_symlink())
                .collect()
        })
        .unwrap_or_default()
}

fn newest_mtime(path: &Path) -> Result<i64, String> {
    let mut newest = fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let modified = fs::symlink_metadata(entry.path())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(newest);
        newest = newest.max(modified);
    }
    Ok(newest)
}
fn old(path: &Path, ctx: &ScanContext, days: i64) -> bool {
    newest_mtime(path)
        .map(|m| m <= ctx.now - days * 86400)
        .unwrap_or(false)
}

struct SimplePlugin {
    id: &'static str,
    category: Category,
    risk: Risk,
    roots: Vec<(PathBuf, i64)>,
    reason: &'static str,
    exclude: HashSet<String>,
}
impl CleanerPlugin for SimplePlugin {
    fn id(&self) -> &'static str {
        self.id
    }
    fn discover(
        &self,
        ctx: &ScanContext,
        cancel: &AtomicBool,
    ) -> Result<Vec<RawCandidate>, String> {
        let mut result = Vec::new();
        for (relative, days) in &self.roots {
            for path in children(&ctx.home.join(relative)) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let lower = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if self.exclude.iter().any(|x| lower.contains(x))
                    || (self.id == "old-logs" && lower == "diagnosticreports")
                {
                    continue;
                }
                if old(&path, ctx, *days) {
                    result.push(raw(
                        self.id,
                        self.category,
                        self.risk,
                        path,
                        format!("{}（超过 {} 天未变化）", self.reason, days),
                    ));
                }
            }
        }
        Ok(result)
    }
}

struct Downloads;
impl CleanerPlugin for Downloads {
    fn id(&self) -> &'static str {
        "download-archives"
    }
    fn discover(&self, ctx: &ScanContext, _: &AtomicBool) -> Result<Vec<RawCandidate>, String> {
        Ok(children(&ctx.home.join("Downloads"))
            .into_iter()
            .filter(|p| {
                p.is_file()
                    && matches!(
                        p.extension()
                            .and_then(|x| x.to_str())
                            .map(|x| x.to_lowercase())
                            .as_deref(),
                        Some("dmg" | "pkg" | "zip")
                    )
                    && old(p, ctx, 30)
            })
            .map(|p| {
                raw(
                    self.id(),
                    Category::DownloadLeftovers,
                    Risk::Review,
                    p,
                    "超过 30 天的安装包或压缩包，请确认不再需要",
                )
            })
            .collect())
    }
}

#[derive(Clone, Debug)]
struct VersionKey {
    numbers: Vec<u64>,
    prerelease: Option<String>,
}
impl PartialEq for VersionKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for VersionKey {}
impl Ord for VersionKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let length = self.numbers.len().max(other.numbers.len());
        for index in 0..length {
            let order = self
                .numbers
                .get(index)
                .unwrap_or(&0)
                .cmp(other.numbers.get(index).unwrap_or(&0));
            if !order.is_eq() {
                return order;
            }
        }
        match (&self.prerelease, &other.prerelease) {
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
            (None, None) => std::cmp::Ordering::Equal,
        }
    }
}
impl PartialOrd for VersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
fn parse_version(value: &str) -> Option<VersionKey> {
    let (main, prerelease) = value
        .split_once('-')
        .map(|(a, b)| (a, Some(b.to_string())))
        .unwrap_or((value, None));
    let numbers = main
        .split('.')
        .map(|x| x.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    Some(VersionKey {
        numbers,
        prerelease,
    })
}

#[derive(Clone)]
struct Ext {
    path: PathBuf,
    publisher: String,
    name: String,
    version: VersionKey,
    version_text: String,
    target: String,
    mtime: i64,
}
struct VscodeDuplicates;
impl CleanerPlugin for VscodeDuplicates {
    fn id(&self) -> &'static str {
        "vscode-duplicates"
    }
    fn discover(&self, ctx: &ScanContext, _: &AtomicBool) -> Result<Vec<RawCandidate>, String> {
        let base = ctx.home.join(".vscode/extensions");
        let obsolete: HashSet<String> = fs::read_to_string(base.join(".obsolete"))
            .ok()
            .and_then(|s| serde_json::from_str::<HashMap<String, bool>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, v)| *v)
            .map(|(k, _)| k)
            .collect();
        let mut groups: HashMap<(String, String, String), Vec<Ext>> = HashMap::new();
        for path in children(&base) {
            let Ok(text) = fs::read_to_string(path.join("package.json")) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let (Some(publisher), Some(name), Some(version_text)) = (
                v["publisher"].as_str(),
                v["name"].as_str(),
                v["version"].as_str(),
            ) else {
                continue;
            };
            let Some(version) = parse_version(version_text) else {
                continue;
            };
            let target = v["__metadata"]["targetPlatform"]
                .as_str()
                .unwrap_or("universal")
                .to_string();
            let mtime = fs::metadata(&path).map(|m| m.mtime()).unwrap_or(0);
            groups
                .entry((
                    publisher.to_lowercase(),
                    name.to_lowercase(),
                    target.clone(),
                ))
                .or_default()
                .push(Ext {
                    path,
                    publisher: publisher.into(),
                    name: name.into(),
                    version,
                    version_text: version_text.into(),
                    target,
                    mtime,
                });
        }
        let mut out = Vec::new();
        for mut versions in groups.into_values() {
            if versions.len() < 2 {
                continue;
            }
            versions.sort_by(|a, b| (&b.version, b.mtime).cmp(&(&a.version, a.mtime)));
            let kept = versions[0].version_text.clone();
            for ext in versions.into_iter().skip(1) {
                let is_obsolete = ext
                    .path
                    .file_name()
                    .map(|n| obsolete.contains(&n.to_string_lossy().to_string()))
                    .unwrap_or(false);
                let risk = if is_obsolete { Risk::Low } else { Risk::Review };
                out.push(raw(
                    self.id(),
                    Category::DeveloperCaches,
                    risk,
                    ext.path,
                    format!(
                        "VS Code 扩展 {}.{} 旧版本 {}；保留 {}（{}）{}",
                        ext.publisher,
                        ext.name,
                        ext.version_text,
                        kept,
                        ext.target,
                        if is_obsolete {
                            "，已标记 obsolete"
                        } else {
                            "，请审查"
                        }
                    ),
                ));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn semantic_versions_compare_numerically_and_release_wins() {
        assert!(parse_version("1.10.0").unwrap() > parse_version("1.9.9").unwrap());
        assert_eq!(
            parse_version("1.2").unwrap(),
            parse_version("1.2.0").unwrap()
        );
        assert!(parse_version("2.0.0").unwrap() > parse_version("2.0.0-beta.1").unwrap());
    }

    #[test]
    fn vscode_keeps_highest_and_marks_obsolete_old_version_low_risk() {
        let temp = tempdir().unwrap();
        let base = temp.path().join(".vscode/extensions");
        fs::create_dir_all(&base).unwrap();
        for version in ["1.9.0", "1.10.0"] {
            let dir = base.join(format!("acme.tool-{version}"));
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("package.json"),format!(r#"{{"publisher":"acme","name":"tool","version":"{version}","__metadata":{{"targetPlatform":"darwin-arm64"}}}}"#)).unwrap();
        }
        fs::write(base.join(".obsolete"), r#"{"acme.tool-1.9.0":true}"#).unwrap();
        let ctx = ScanContext {
            home: temp.path().into(),
            now: i64::MAX / 2,
            project_roots: vec![],
        };
        let found = VscodeDuplicates
            .discover(&ctx, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("acme.tool-1.9.0"));
        assert_eq!(found[0].risk, Risk::Low);
        assert!(found[0].reason.contains("保留 1.10.0"));
    }

    #[test]
    fn project_dependencies_are_review_only_and_do_not_follow_links() {
        let temp = tempdir().unwrap();
        let projects = temp.path().join("Code");
        let app = projects.join("app");
        fs::create_dir_all(app.join("node_modules/pkg/node_modules")).unwrap();
        fs::create_dir(app.join(".venv")).unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir(outside.path().join("node_modules")).unwrap();
        symlink(outside.path(), projects.join("linked")).unwrap();
        let ctx = ScanContext {
            home: temp.path().into(),
            now: 0,
            project_roots: vec![projects],
        };
        let found = ProjectDependencies
            .discover(&ctx, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|c| c.risk == Risk::Review));
        assert!(
            found
                .iter()
                .all(|c| !c.path.to_string_lossy().contains("linked"))
        );
    }
}

struct ProjectDependencies;
impl CleanerPlugin for ProjectDependencies {
    fn id(&self) -> &'static str {
        "project-dependencies"
    }
    fn discover(
        &self,
        ctx: &ScanContext,
        cancel: &AtomicBool,
    ) -> Result<Vec<RawCandidate>, String> {
        let names = ["node_modules", ".venv", "venv"];
        let mut out = Vec::new();
        for name in names {
            let p = ctx.home.join(name);
            if p.is_dir() && !p.is_symlink() {
                out.push(raw(
                    self.id(),
                    Category::ProjectDependencies,
                    Risk::Review,
                    p,
                    format!("用户目录直属依赖 {name}，删除后需要重新安装"),
                ));
            }
        }
        for root in &ctx.project_roots {
            let mut stack = vec![(root.clone(), 0usize)];
            while let Some((current, depth)) = stack.pop() {
                if cancel.load(Ordering::Relaxed) || depth >= 8 {
                    continue;
                }
                let Ok(entries) = fs::read_dir(&current) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_symlink() || !path.is_dir() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if matches!(
                        name.as_str(),
                        ".git" | ".hg" | ".svn" | "Library" | ".Trash" | "CloudStorage"
                    ) {
                        continue;
                    }
                    if names.contains(&name.as_str()) {
                        out.push(raw(
                            self.id(),
                            Category::ProjectDependencies,
                            Risk::Review,
                            path.clone(),
                            format!(
                                "项目 {} 的 {}，删除后需要重新安装依赖",
                                path.parent().unwrap_or(root).display(),
                                name
                            ),
                        ));
                    } else {
                        stack.push((path, depth + 1));
                    }
                }
            }
        }
        Ok(out)
    }
}

pub fn builtin_plugins() -> Vec<Box<dyn CleanerPlugin>> {
    let excluded = [
        "safari",
        "chrome",
        "firefox",
        "mozilla",
        "cloudkit",
        "keychain",
        "credential",
        "1password",
        "lastpass",
        "bitwarden",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    vec![
        Box::new(SimplePlugin {
            id: "user-caches",
            category: Category::ApplicationCaches,
            risk: Risk::Low,
            roots: vec![("Library/Caches".into(), 30)],
            reason: "可由应用重新生成的用户缓存",
            exclude: excluded,
        }),
        Box::new(SimplePlugin {
            id: "developer-caches",
            category: Category::DeveloperCaches,
            risk: Risk::Low,
            roots: vec![
                (".cache".into(), 30),
                (".npm/_cacache".into(), 30),
                ("Library/Caches/pip".into(), 30),
                ("Library/Developer/Xcode/DerivedData".into(), 14),
            ],
            reason: "可再生成的开发工具缓存",
            exclude: HashSet::new(),
        }),
        Box::new(VscodeDuplicates),
        Box::new(ProjectDependencies),
        Box::new(SimplePlugin {
            id: "old-logs",
            category: Category::Logs,
            risk: Risk::Low,
            roots: vec![("Library/Logs".into(), 30)],
            reason: "用户日志",
            exclude: HashSet::new(),
        }),
        Box::new(SimplePlugin {
            id: "crash-reports",
            category: Category::CrashReports,
            risk: Risk::Low,
            roots: vec![("Library/Logs/DiagnosticReports".into(), 14)],
            reason: "崩溃诊断报告",
            exclude: HashSet::new(),
        }),
        Box::new(Downloads),
    ]
}
