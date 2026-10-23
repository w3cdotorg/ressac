//! Off Linux, ressac-kde is a stub (master plan, "ressac-kde").
#![cfg(not(target_os = "linux"))]

#[test]
fn off_linux_the_binary_says_so_and_exits_1() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ressac-kde"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "ressac-kde runs on Linux only.\n"
    );
    assert!(out.stdout.is_empty());
}
