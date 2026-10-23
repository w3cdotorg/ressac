//! `logging::init` installs a global subscriber: these tests run in their own process.

#[test]
fn init_logs_to_a_daily_file_in_the_directory_it_creates() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("state/ressac/logs");
    let guard =
        ressac_core::logging::init(&logs, false).expect("a writable directory gives a guard");
    tracing::error!("logging test line");
    drop(guard); // flushes the non-blocking writer
    let names: Vec<String> = std::fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with("ressac.log"), "{names:?}");
    let text = std::fs::read_to_string(logs.join(&names[0])).unwrap();
    assert!(text.contains("logging test line"), "{text}");
    assert!(
        !text.contains('\u{1b}'),
        "no colour codes in the file: {text}"
    );
}

#[test]
fn a_directory_that_cannot_be_created_gives_no_guard() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, b"").unwrap();
    assert!(ressac_core::logging::init(&file.join("logs"), false).is_none());
}
