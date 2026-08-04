use crate::model::*;
use crate::plugins::{CleanerPlugin, ScanContext, builtin_plugins};
use crate::settings::Settings;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

const SENSITIVE: &[&str] = &[
    ".ssh",
    ".gnupg",
    "Keychains",
    "credentials",
    "secrets",
    "CloudStorage",
    "Mobile Documents",
    "Application Support",
    "Containers",
    "Group Containers",
];

pub fn disk_stats(path: &Path) -> DiskStats {
    DiskStats {
        total_bytes: fs2::total_space(path).unwrap_or(0),
        free_bytes: fs2::available_space(path).unwrap_or(0),
    }
}

pub fn is_sensitive(path: &Path, home: &Path) -> bool {
    path.strip_prefix(home)
        .map(|p| {
            p.components()
                .any(|c| SENSITIVE.iter().any(|s| c.as_os_str() == *s))
        })
        .unwrap_or(true)
}

pub fn snapshot(path: &Path) -> Result<Snapshot, String> {
    snapshot_with_visit(path, &mut |_| {})
}

pub fn directory_size_strict(path: &Path) -> Result<u64, String> {
    let mut size = 0u64;
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        if metadata.is_file() {
            size = size.saturating_add(metadata.len());
        }
    }
    Ok(size)
}

fn snapshot_with_visit(path: &Path, visit: &mut dyn FnMut(&Path)) -> Result<Snapshot, String> {
    let root_meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if root_meta.file_type().is_symlink() {
        return Err("候选根目录不能是符号链接".into());
    }
    let mut entries: Vec<PathBuf> = WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.path().to_path_buf())
        .collect();
    entries.sort();
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    for entry in entries {
        visit(&entry);
        let meta = fs::symlink_metadata(&entry).map_err(|e| format!("{}: {e}", entry.display()))?;
        let rel = entry.strip_prefix(path).unwrap_or(Path::new("."));
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0]);
        for value in [
            meta.dev(),
            meta.ino(),
            meta.mode() as u64,
            meta.len(),
            meta.mtime_nsec() as u64,
        ] {
            hasher.update(value.to_le_bytes());
        }
        if meta.is_file() {
            size = size.saturating_add(meta.len());
        }
    }
    Ok(Snapshot {
        device: root_meta.dev(),
        inode: root_meta.ino(),
        mode: root_meta.mode(),
        size,
        digest: format!("{:x}", hasher.finalize()),
    })
}

fn make_candidate(
    raw: crate::plugins::RawCandidate,
    home: &Path,
    visit: &mut dyn FnMut(&Path),
) -> Option<CandidateInternal> {
    if raw.path.is_symlink() {
        return None;
    }
    let path = raw.path.canonicalize().ok()?;
    let canonical_home = home.canonicalize().ok()?;
    if !path.starts_with(&canonical_home) || is_sensitive(&path, &canonical_home) {
        return None;
    }
    let snap = snapshot_with_visit(&path, visit).ok()?;
    if snap.size == 0 {
        return None;
    }
    let modified_at = fs::symlink_metadata(&path).ok()?.mtime();
    let mut id_hash = Sha256::new();
    id_hash.update(raw.plugin.as_bytes());
    id_hash.update([0]);
    id_hash.update(path.to_string_lossy().as_bytes());
    let id = format!("{:x}", id_hash.finalize())[..12].to_string();
    let public = Candidate {
        id,
        plugin: raw.plugin,
        category: raw.category,
        risk: raw.risk,
        path: path.to_string_lossy().into_owned(),
        size_bytes: snap.size,
        modified_at,
        reason: raw.reason,
        action: "move_to_trash".into(),
    };
    Some(CandidateInternal {
        public,
        snapshot: snap,
        path,
    })
}

pub fn scan<F>(
    scan_id: String,
    home: PathBuf,
    settings: Settings,
    categories: Vec<Category>,
    cancel: Arc<AtomicBool>,
    mut progress: F,
) -> Result<ScanSession, String>
where
    F: FnMut(ScanEvent),
{
    scan_with_plugins(
        scan_id,
        home,
        settings,
        categories,
        cancel,
        builtin_plugins(),
        &mut progress,
    )
}

fn scan_with_plugins<F>(
    scan_id: String,
    home: PathBuf,
    settings: Settings,
    categories: Vec<Category>,
    cancel: Arc<AtomicBool>,
    plugins: Vec<Box<dyn CleanerPlugin>>,
    mut progress: F,
) -> Result<ScanSession, String>
where
    F: FnMut(ScanEvent),
{
    let categories = validate_categories(categories)?;
    let plugins: Vec<_> = plugins
        .into_iter()
        .filter(|plugin| categories.contains(&plugin.category()))
        .collect();
    let total_plugins = plugins.len();
    progress(ScanEvent::Started {
        scan_id: scan_id.clone(),
        total_plugins,
    });
    let context = ScanContext {
        home: home.clone(),
        now: Utc::now().timestamp(),
        project_roots: settings.project_roots.iter().map(PathBuf::from).collect(),
    };
    let mut found: HashMap<String, CandidateInternal> = HashMap::new();
    let mut warnings = Vec::new();
    let mut total_bytes = 0u64;
    for (offset, plugin) in plugins.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("__cancelled__".into());
        }
        let plugin_id = plugin.id().to_string();
        let category = plugin.category();
        let plugin_index = offset + 1;
        progress(ScanEvent::PluginStarted {
            plugin: plugin_id.clone(),
            category,
            plugin_index,
            total_plugins,
        });
        let mut visited = 0usize;
        let mut last_emit: Option<Instant> = None;
        let mut visit = |path: &Path| {
            visited += 1;
            let now = Instant::now();
            if last_emit
                .map(|last| now.duration_since(last) >= Duration::from_millis(100))
                .unwrap_or(true)
            {
                progress(ScanEvent::Progress {
                    plugin: plugin_id.clone(),
                    category,
                    path: path.to_string_lossy().into_owned(),
                    plugin_index,
                    total_plugins,
                    visited,
                    found: found.len(),
                    bytes: total_bytes,
                });
                last_emit = Some(now);
            }
        };
        let discovered = plugin.discover(&context, &cancel, &mut visit);
        drop(visit);
        match discovered {
            Ok(raws) => {
                for raw in raws {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("__cancelled__".into());
                    }
                    let mut candidate_visit = |path: &Path| {
                        visited += 1;
                        let now = Instant::now();
                        if last_emit
                            .map(|last| now.duration_since(last) >= Duration::from_millis(100))
                            .unwrap_or(true)
                        {
                            progress(ScanEvent::Progress {
                                plugin: plugin_id.clone(),
                                category,
                                path: path.to_string_lossy().into_owned(),
                                plugin_index,
                                total_plugins,
                                visited,
                                found: found.len(),
                                bytes: total_bytes,
                            });
                            last_emit = Some(now);
                        }
                    };
                    let candidate = make_candidate(raw, &home, &mut candidate_visit);
                    drop(candidate_visit);
                    if let Some(candidate) = candidate {
                        let path = candidate.path.to_string_lossy().into_owned();
                        let overlaps: Vec<String> = found
                            .keys()
                            .filter(|existing| {
                                let existing = Path::new(existing);
                                candidate.path == existing
                                    || candidate.path.starts_with(existing)
                                    || existing.starts_with(&candidate.path)
                            })
                            .cloned()
                            .collect();
                        if overlaps.is_empty() {
                            total_bytes += candidate.public.size_bytes;
                            found.insert(path.clone(), candidate);
                        } else if overlaps
                            .iter()
                            .all(|existing| Path::new(existing).starts_with(&candidate.path))
                        {
                            for existing in overlaps {
                                if let Some(old) = found.remove(&existing) {
                                    total_bytes = total_bytes.saturating_sub(old.public.size_bytes);
                                }
                            }
                            total_bytes += candidate.public.size_bytes;
                            found.insert(path.clone(), candidate);
                        }
                    }
                }
            }
            Err(error) => warnings.push(format!("{plugin_id}: {error}")),
        }
        progress(ScanEvent::PluginCompleted {
            plugin: plugin_id,
            category,
            plugin_index,
            total_plugins,
            visited,
            found: found.len(),
            bytes: total_bytes,
        });
    }
    let mut values: Vec<_> = found.into_values().collect();
    values.sort_by(|a, b| {
        (
            a.public.risk,
            a.public.category.order(),
            std::cmp::Reverse(a.public.size_bytes),
            &a.public.path,
        )
            .cmp(&(
                b.public.risk,
                b.public.category.order(),
                std::cmp::Reverse(b.public.size_bytes),
                &b.public.path,
            ))
    });
    let report = ScanReport {
        scan_id,
        generated_at: Utc::now().to_rfc3339(),
        disk: disk_stats(&home),
        candidates: values.iter().map(|c| c.public.clone()).collect(),
        warnings,
    };
    let candidates = values
        .into_iter()
        .map(|c| (c.public.id.clone(), c))
        .collect();
    Ok(ScanSession {
        report,
        candidates,
        categories,
    })
}

pub fn validate_categories(categories: Vec<Category>) -> Result<Vec<Category>, String> {
    if categories.is_empty() {
        return Err("请至少选择一个扫描类别".into());
    }
    let supported: HashSet<_> = [
        Category::ApplicationCaches,
        Category::DeveloperCaches,
        Category::ProjectDependencies,
        Category::Logs,
        Category::CrashReports,
        Category::DownloadLeftovers,
    ]
    .into_iter()
    .collect();
    if categories
        .iter()
        .any(|category| !supported.contains(category))
    {
        return Err("包含不支持的扫描类别".into());
    }
    let mut unique = Vec::new();
    for category in categories {
        if !unique.contains(&category) {
            unique.push(category);
        }
    }
    Ok(unique)
}

pub fn revalidate(candidate: &CandidateInternal, home: &Path, trash: &Path) -> Result<(), String> {
    let canonical_home = home.canonicalize().map_err(|e| e.to_string())?;
    let canonical_path = candidate.path.canonicalize().map_err(|e| e.to_string())?;
    if candidate.path.is_symlink()
        || canonical_path != candidate.path
        || !canonical_path.starts_with(&canonical_home)
        || is_sensitive(&canonical_path, &canonical_home)
    {
        return Err("路径越界、敏感或为符号链接".into());
    }
    let current = snapshot(&candidate.path)?;
    if current.device != candidate.snapshot.device
        || current.inode != candidate.snapshot.inode
        || current.mode != candidate.snapshot.mode
    {
        return Err("文件身份在扫描后发生变化".into());
    }
    if current.size != candidate.snapshot.size || current.digest != candidate.snapshot.digest {
        return Err("内容在扫描后发生变化".into());
    }
    let trash_device = fs::metadata(trash).map_err(|e| e.to_string())?.dev();
    if current.device != trash_device {
        return Err("候选与废纸篓不在同一文件系统".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{RawCandidate, ScanContext};
    use std::os::unix::fs::symlink;
    use std::sync::atomic::AtomicUsize;
    use tempfile::tempdir;
    struct Broken;
    impl CleanerPlugin for Broken {
        fn id(&self) -> &'static str {
            "broken"
        }
        fn category(&self) -> Category {
            Category::Logs
        }
        fn discover(
            &self,
            _: &ScanContext,
            _: &AtomicBool,
            _: &mut dyn FnMut(&Path),
        ) -> Result<Vec<RawCandidate>, String> {
            Err("boom".into())
        }
    }
    struct RecordingPlugin {
        id: &'static str,
        category: Category,
        calls: Arc<AtomicUsize>,
        visits: usize,
    }
    impl CleanerPlugin for RecordingPlugin {
        fn id(&self) -> &'static str {
            self.id
        }
        fn category(&self) -> Category {
            self.category
        }
        fn discover(
            &self,
            ctx: &ScanContext,
            cancel: &AtomicBool,
            visit: &mut dyn FnMut(&Path),
        ) -> Result<Vec<RawCandidate>, String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            for index in 0..self.visits {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                visit(&ctx.home.join(format!("path-{index}")));
            }
            Ok(vec![])
        }
    }
    #[test]
    fn snapshot_does_not_count_or_read_symlink_target() {
        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("large"), vec![0u8; 4096]).unwrap();
        let candidate = home.path().join("candidate");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("small"), b"abc").unwrap();
        symlink(outside.path().join("large"), candidate.join("link")).unwrap();
        let snap = snapshot(&candidate).unwrap();
        assert_eq!(snap.size, 3);
    }
    #[test]
    fn strict_directory_size_counts_files_but_not_symlink_targets() {
        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let directory = home.path().join("directory");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(directory.join("nested")).unwrap();
        fs::write(directory.join("first"), b"123").unwrap();
        fs::write(directory.join("nested/second"), b"4567").unwrap();
        fs::write(outside.path().join("large"), vec![0u8; 1024]).unwrap();
        symlink(outside.path().join("large"), directory.join("link")).unwrap();
        assert_eq!(directory_size_strict(&directory).unwrap(), 7);
        assert!(directory_size_strict(&home.path().join("missing")).is_err());
    }
    #[test]
    fn cancelled_scan_stops_before_plugins() {
        let home = tempdir().unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let result = scan_with_plugins(
            "id".into(),
            home.path().into(),
            Settings {
                project_roots: vec![],
            },
            vec![Category::Logs],
            cancel,
            vec![Box::new(Broken)],
            |_| {},
        );
        assert_eq!(result.unwrap_err(), "__cancelled__");
    }
    #[test]
    fn plugin_failure_becomes_warning() {
        let home = tempdir().unwrap();
        let result = scan_with_plugins(
            "id".into(),
            home.path().into(),
            Settings {
                project_roots: vec![],
            },
            vec![Category::Logs],
            Arc::new(AtomicBool::new(false)),
            vec![Box::new(Broken)],
            |_| {},
        )
        .unwrap();
        assert_eq!(result.report.warnings, vec!["broken: boom"]);
        assert!(result.report.candidates.is_empty());
    }
    #[test]
    fn empty_and_unsupported_categories_are_rejected() {
        assert!(validate_categories(vec![]).unwrap_err().contains("至少"));
        assert!(
            validate_categories(vec![Category::Other])
                .unwrap_err()
                .contains("不支持")
        );
    }
    #[test]
    fn selected_category_runs_every_matching_plugin_only() {
        let home = tempdir().unwrap();
        let app_calls = Arc::new(AtomicUsize::new(0));
        let log_calls = Arc::new(AtomicUsize::new(0));
        let plugins: Vec<Box<dyn CleanerPlugin>> = vec![
            Box::new(RecordingPlugin {
                id: "app-a",
                category: Category::ApplicationCaches,
                calls: app_calls.clone(),
                visits: 0,
            }),
            Box::new(RecordingPlugin {
                id: "app-b",
                category: Category::ApplicationCaches,
                calls: app_calls.clone(),
                visits: 0,
            }),
            Box::new(RecordingPlugin {
                id: "logs",
                category: Category::Logs,
                calls: log_calls.clone(),
                visits: 0,
            }),
        ];
        scan_with_plugins(
            "id".into(),
            home.path().into(),
            Settings {
                project_roots: vec![],
            },
            vec![Category::ApplicationCaches],
            Arc::new(AtomicBool::new(false)),
            plugins,
            |_| {},
        )
        .unwrap();
        assert_eq!(app_calls.load(Ordering::Relaxed), 2);
        assert_eq!(log_calls.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn progress_events_are_ordered_and_fast_visits_are_throttled() {
        let home = tempdir().unwrap();
        let plugin = RecordingPlugin {
            id: "paths",
            category: Category::Logs,
            calls: Arc::new(AtomicUsize::new(0)),
            visits: 8,
        };
        let mut events = Vec::new();
        scan_with_plugins(
            "id".into(),
            home.path().into(),
            Settings {
                project_roots: vec![],
            },
            vec![Category::Logs],
            Arc::new(AtomicBool::new(false)),
            vec![Box::new(plugin)],
            |event| events.push(event),
        )
        .unwrap();
        assert!(matches!(
            events[0],
            ScanEvent::Started {
                total_plugins: 1,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            ScanEvent::PluginStarted {
                plugin_index: 1,
                total_plugins: 1,
                ..
            }
        ));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ScanEvent::Progress { .. }))
                .count(),
            1
        );
        assert!(matches!(
            events.last().unwrap(),
            ScanEvent::PluginCompleted {
                plugin_index: 1,
                total_plugins: 1,
                ..
            }
        ));
    }
    #[test]
    fn candidates_sort_by_risk_category_then_size_descending() {
        let home = tempdir().unwrap();
        let a = home.path().join("a");
        let b = home.path().join("b");
        fs::write(&a, vec![0u8; 10]).unwrap();
        fs::write(&b, vec![0u8; 20]).unwrap();
        struct Fixed {
            items: Vec<(PathBuf, Risk)>,
        }
        impl CleanerPlugin for Fixed {
            fn id(&self) -> &'static str {
                "fixed"
            }
            fn category(&self) -> Category {
                Category::ApplicationCaches
            }
            fn discover(
                &self,
                _: &ScanContext,
                _: &AtomicBool,
                _: &mut dyn FnMut(&Path),
            ) -> Result<Vec<RawCandidate>, String> {
                Ok(self
                    .items
                    .iter()
                    .map(|(p, r)| RawCandidate {
                        plugin: "fixed".into(),
                        category: Category::ApplicationCaches,
                        risk: *r,
                        path: p.clone(),
                        reason: "test".into(),
                    })
                    .collect())
            }
        }
        let plugin = Fixed {
            items: vec![(a, Risk::Low), (b, Risk::Low)],
        };
        let result = scan_with_plugins(
            "id".into(),
            home.path().into(),
            Settings {
                project_roots: vec![],
            },
            vec![Category::ApplicationCaches],
            Arc::new(AtomicBool::new(false)),
            vec![Box::new(plugin)],
            |_| {},
        )
        .unwrap();
        assert_eq!(
            result
                .report
                .candidates
                .iter()
                .map(|c| c.size_bytes)
                .collect::<Vec<_>>(),
            vec![20, 10]
        );
    }
    #[test]
    #[ignore = "只读扫描真实用户目录，供人工冒烟验收"]
    fn real_home_read_only_smoke() {
        let home = dirs::home_dir().expect("home");
        let settings = crate::settings::defaults(&home);
        let mut progress_events = 0usize;
        let result = scan(
            "real-smoke".into(),
            home,
            settings,
            vec![
                Category::ApplicationCaches,
                Category::DeveloperCaches,
                Category::ProjectDependencies,
                Category::Logs,
                Category::CrashReports,
                Category::DownloadLeftovers,
            ],
            Arc::new(AtomicBool::new(false)),
            |event| {
                if matches!(event, ScanEvent::Progress { .. }) {
                    progress_events += 1;
                }
            },
        )
        .unwrap();
        assert!(progress_events > 0);
        eprintln!(
            "candidates={} bytes={} warnings={} progress_events={}",
            result.report.candidates.len(),
            result
                .report
                .candidates
                .iter()
                .map(|c| c.size_bytes)
                .sum::<u64>(),
            result.report.warnings.len(),
            progress_events
        );
    }
}
