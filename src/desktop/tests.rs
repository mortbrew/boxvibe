use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = crate::config::scratch_dir(&format!("boxvibe-desktop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn installs_and_removes_its_own_entry() {
    let dir = temp("own").join("applications");
    assert!(!installed(&dir));
    let path = install(&dir, Path::new("/usr/bin/boxvibe")).unwrap();
    assert!(installed(&dir));
    let text = std::fs::read_to_string(&path).unwrap();
    for line in [
        "Type=Application",
        "Name=boxvibe",
        "Exec=/usr/bin/boxvibe",
        "Terminal=true",
        "Categories=Audio;AudioVideo;Player;",
        "Keywords=pandora;music;radio;",
        MARKER,
    ] {
        assert!(text.lines().any(|l| l == line), "{line} in\n{text}");
    }
    remove(&dir).unwrap();
    assert!(!installed(&dir));
    std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn never_deletes_or_overwrites_a_file_it_didnt_write() {
    let dir = temp("theirs");
    std::fs::create_dir_all(&dir).unwrap();
    let mine = "[Desktop Entry]\nType=Application\nName=boxvibe (mine)\nExec=boxvibe\n";
    std::fs::write(path_in(&dir), mine).unwrap();
    assert!(remove(&dir).is_err());
    assert!(install(&dir, Path::new("/x/boxvibe")).is_err());
    assert_eq!(std::fs::read_to_string(path_in(&dir)).unwrap(), mine);
    assert!(remove(&temp("none")).is_err(), "nothing there");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn exec_paths_are_quoted_per_the_spec() {
    assert_eq!(exec_arg(Path::new("/home/t/.cargo/bin/boxvibe")), "/home/t/.cargo/bin/boxvibe");
    assert_eq!(exec_arg(Path::new("/home/t/my apps/boxvibe")), "\"/home/t/my apps/boxvibe\"");
    assert_eq!(exec_arg(Path::new("/a b/$x\"y")), r#""/a b/\\$x\\"y""#);
    assert_eq!(exec_arg(Path::new("/opt/100%/boxvibe")), "/opt/100%%/boxvibe");
}
