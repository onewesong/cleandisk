use crate::model::{CandidateInternal, CleanFailure};
use crate::scanner;
use crate::trash_backend::TrashBackend;
use std::path::Path;

#[derive(Debug, Default, PartialEq)]
pub struct CleanBatchOutcome {
    pub moved_count: usize,
    pub moved_bytes: u64,
    pub failed_bytes: u64,
    pub failures: Vec<CleanFailure>,
}

pub fn clean_batch<F>(
    backend: &dyn TrashBackend,
    selected: Vec<CandidateInternal>,
    home: &Path,
    trash: &Path,
    mut progress: F,
) -> CleanBatchOutcome
where
    F: FnMut(&str, bool, &str),
{
    let mut outcome = CleanBatchOutcome::default();
    for candidate in selected {
        let result = scanner::revalidate(&candidate, home, trash)
            .and_then(|_| backend.move_to_trash(&candidate.path));
        match result {
            Ok(()) => {
                outcome.moved_count += 1;
                outcome.moved_bytes += candidate.public.size_bytes;
                progress(&candidate.public.id, true, "已移入废纸篓");
            }
            Err(message) => {
                outcome.failed_bytes += candidate.public.size_bytes;
                outcome.failures.push(CleanFailure {
                    id: candidate.public.id.clone(),
                    path: candidate.public.path.clone(),
                    message: message.clone(),
                    size_bytes: candidate.public.size_bytes,
                });
                progress(&candidate.public.id, false, &message);
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Candidate, Category, Risk};
    use crate::scanner;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::tempdir;

    #[derive(Default)]
    struct FakeTrash {
        moved: Mutex<Vec<PathBuf>>,
        fail_name: Option<String>,
    }
    impl TrashBackend for FakeTrash {
        fn move_to_trash(&self, path: &Path) -> Result<(), String> {
            if self.fail_name.as_deref() == path.file_name().and_then(|v| v.to_str()) {
                return Err("模拟移动失败".into());
            }
            self.moved.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }
    }

    fn candidate(path: &Path, id: &str) -> CandidateInternal {
        let path = path.canonicalize().unwrap();
        let snapshot = scanner::snapshot(&path).unwrap();
        CandidateInternal {
            public: Candidate {
                id: id.into(),
                plugin: "test".into(),
                category: Category::Other,
                risk: Risk::Low,
                path: path.display().to_string(),
                size_bytes: snapshot.size,
                modified_at: 0,
                reason: "test".into(),
                action: "move_to_trash".into(),
            },
            snapshot,
            path,
        }
    }

    #[test]
    fn moves_only_selected_and_reports_partial_failure() {
        let temp = tempdir().unwrap();
        let trash = temp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let unselected = temp.path().join("unselected");
        fs::write(&first, b"1234").unwrap();
        fs::write(&second, b"123456").unwrap();
        fs::write(&unselected, b"untouched").unwrap();
        let backend = FakeTrash {
            moved: Mutex::new(Vec::new()),
            fail_name: Some("second".into()),
        };
        let outcome = clean_batch(
            &backend,
            vec![candidate(&first, "a"), candidate(&second, "b")],
            temp.path(),
            &trash,
            |_, _, _| {},
        );
        assert_eq!(outcome.moved_count, 1);
        assert_eq!(outcome.moved_bytes, 4);
        assert_eq!(outcome.failed_bytes, 6);
        assert_eq!(
            backend.moved.lock().unwrap().as_slice(),
            &[first.canonicalize().unwrap()]
        );
        assert!(unselected.exists());
    }

    #[test]
    fn changed_candidate_is_not_sent_to_backend() {
        let temp = tempdir().unwrap();
        let trash = temp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let path = temp.path().join("changing");
        fs::write(&path, b"before").unwrap();
        let item = candidate(&path, "changed");
        fs::write(&path, b"after-content").unwrap();
        let backend = FakeTrash::default();
        let outcome = clean_batch(&backend, vec![item], temp.path(), &trash, |_, _, _| {});
        assert_eq!(outcome.moved_count, 0);
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.failures[0].message.contains("变化"));
        assert!(backend.moved.lock().unwrap().is_empty());
    }
}
