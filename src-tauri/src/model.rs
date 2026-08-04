use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Review,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    ApplicationCaches,
    DeveloperCaches,
    ProjectDependencies,
    Logs,
    CrashReports,
    DownloadLeftovers,
    Other,
}

impl Category {
    pub fn order(self) -> u8 {
        match self {
            Self::ApplicationCaches => 0,
            Self::DeveloperCaches => 1,
            Self::ProjectDependencies => 2,
            Self::Logs => 3,
            Self::CrashReports => 4,
            Self::DownloadLeftovers => 5,
            Self::Other => 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub plugin: String,
    pub category: Category,
    pub risk: Risk,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub device: u64,
    pub inode: u64,
    pub mode: u32,
    pub size: u64,
    pub digest: String,
}

#[derive(Debug, Clone)]
pub struct CandidateInternal {
    pub public: Candidate,
    pub snapshot: Snapshot,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskStats {
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scan_id: String,
    pub generated_at: String,
    pub disk: DiskStats,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScanSession {
    pub report: ScanReport,
    pub candidates: HashMap<String, CandidateInternal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ScanEvent {
    Started {
        scan_id: String,
    },
    Progress {
        plugin: String,
        path: String,
        found: usize,
        bytes: u64,
    },
    Completed {
        report: ScanReport,
    },
    Cancelled {
        scan_id: String,
    },
    Failed {
        scan_id: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CleanEvent {
    Started {
        total: usize,
    },
    ItemCompleted {
        id: String,
        success: bool,
        message: String,
    },
    Rescanning,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleanFailure {
    pub id: String,
    pub path: String,
    pub message: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub moved_count: usize,
    pub moved_bytes: u64,
    pub failed_bytes: u64,
    pub failures: Vec<CleanFailure>,
    pub free_before: u64,
    pub free_after: u64,
    pub trash_before: u64,
    pub trash_after: u64,
    pub refreshed_scan: ScanReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub project_roots: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_json_fixture_round_trips() {
        let source = include_str!("../../fixtures/scan-report.json");
        let report: ScanReport = serde_json::from_str(source).unwrap();
        assert_eq!(report.scan_id, "fixture-scan");
        assert_eq!(report.candidates[0].category, Category::DeveloperCaches);
        let expected: serde_json::Value = serde_json::from_str(source).unwrap();
        let actual = serde_json::to_value(report).unwrap();
        assert_eq!(actual, expected);
    }
}
