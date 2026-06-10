use gravity_wal::{RecoveryVerdict, WalManager};
use serde_json::json;
use std::fs;

#[test]
fn wal_recovery_report_detects_clean_append_streams() {
    let dir = std::env::temp_dir().join(format!("gravity-wal-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let wal = WalManager::new(&dir, true, 128);
    wal.append("order", "BTC-USDx", 1, json!({"order":"a"})).unwrap();
    wal.append("fill", "BTC-USDx", 2, json!({"fill":"b"})).unwrap();
    wal.checkpoint("unit test checkpoint").unwrap();
    let report = wal.recovery_report().unwrap();
    assert!(matches!(report.verdict, RecoveryVerdict::Healthy | RecoveryVerdict::Degraded));
    assert!(report.total_records >= 2);
    assert_eq!(report.malformed_records, 0);
    let _ = fs::remove_dir_all(&dir);
}
