use agent_memory::host::{self, PidState};

#[test]
fn fixed_utc_instants_match_datetime_isoformat() {
    assert_eq!(
        host::isoformat_utc(0, 0).unwrap(),
        "1970-01-01T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(0, 999999).unwrap(),
        "1970-01-01T00:00:00.999999+00:00"
    );
    assert_eq!(
        host::isoformat_utc(951782400, 1).unwrap(),
        "2000-02-29T00:00:00.000001+00:00"
    );
    assert_eq!(
        host::isoformat_utc(946684799, 0).unwrap(),
        "1999-12-31T23:59:59+00:00"
    );
    assert_eq!(
        host::isoformat_utc(946684800, 0).unwrap(),
        "2000-01-01T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(-1, 1).unwrap(),
        "1969-12-31T23:59:59.000001+00:00"
    );
    assert!(host::isoformat_utc(0, 1000000).is_err());
}
#[test]
fn os_uuid_and_host_are_real_and_bounded() {
    let value = host::uuid4().unwrap();
    assert!(agent_memory::lifecycle::identity(
        &agent_memory::codec::json::Value::String(value.clone())
    ));
    assert_eq!(value.len(), 36);
    assert!(!host::hostname().unwrap().is_empty());
    assert_eq!(
        host::pid_state(std::process::id() as i32).unwrap(),
        PidState::Alive
    );
    assert!(host::pid_state(0).is_err());
    assert!(host::pid_state(-1).is_err());
    assert!(
        agent_memory::lifecycle::time_value(&agent_memory::codec::json::Value::String(
            host::utc_now_isoformat().unwrap()
        ))
        .is_ok()
    );
}

#[test]
fn expanduser_matches_posix_home_rules_without_environment_mutation() {
    use std::ffi::OsStr;
    use std::path::Path;
    let expand =
        |path| host::expanduser_with_home(Path::new(path), Some(OsStr::new("/a/home///"))).unwrap();
    assert_eq!(expand("~"), Path::new("/a/home"));
    assert_eq!(expand("~/x"), Path::new("/a/home/x"));
    assert_eq!(expand("a/~/x"), Path::new("a/~/x"));
    assert_eq!(
        host::expanduser_with_home(Path::new("~"), Some(OsStr::new(""))).unwrap(),
        Path::new("/")
    );
    assert_eq!(
        host::expanduser_with_home(Path::new("~/x"), Some(OsStr::new(""))).unwrap(),
        Path::new("/x")
    );
    let missing = "~agent-memory-no-such-user-9d8d035a01234/x";
    assert_eq!(expand(missing), Path::new(missing));
    if let Some(name) = host::current_username().unwrap() {
        let named = Path::new(&format!("~{}", name.to_str().unwrap())).to_path_buf();
        assert_eq!(
            host::expanduser_with_home(&named, None).unwrap(),
            host::expanduser_with_home(Path::new("~"), None).unwrap()
        );
    }
}
