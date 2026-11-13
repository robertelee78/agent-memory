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
    // datetime's year range ends: year 1 and year 9999.
    assert_eq!(
        host::isoformat_utc(-62135596800, 0).unwrap(),
        "0001-01-01T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(253402300799, 0).unwrap(),
        "9999-12-31T23:59:59+00:00"
    );
    assert!(host::isoformat_utc(-62135596801, 0).is_err());
    assert!(host::isoformat_utc(253402300800, 0).is_err());
    // Non-leap centuries: 1900-02-28 is followed by 1900-03-01, likewise 2100.
    assert_eq!(
        host::isoformat_utc(-2203891200, 0).unwrap(),
        "1900-03-01T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(-2203977600, 0).unwrap(),
        "1900-02-28T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(4107542400, 0).unwrap(),
        "2100-03-01T00:00:00+00:00"
    );
    assert_eq!(
        host::isoformat_utc(4107456000, 0).unwrap(),
        "2100-02-28T00:00:00+00:00"
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
    assert_ne!(host::uuid4().unwrap(), value);
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
fn expanduser_matches_pathlib_without_environment_mutation() {
    use std::ffi::OsStr;
    use std::path::Path;
    let home = Some(OsStr::new("/a/home///"));
    let expand = |path| host::expanduser_with_home(Path::new(path), home).unwrap();
    assert_eq!(expand("~"), Path::new("/a/home"));
    assert_eq!(expand("~/x"), Path::new("/a/home/x"));
    assert_eq!(expand("a/~/x"), Path::new("a/~/x"));
    // pathlib drops `.` parts before looking for `~`.
    assert_eq!(expand("./~/x"), Path::new("/a/home/x"));
    // pathlib's textual form: repeated, `.` and trailing separators vanish.
    assert_eq!(expand("/x//y/./z/").as_os_str(), OsStr::new("/x/y/z"));
    assert_eq!(
        host::expanduser_with_home(Path::new("~"), Some(OsStr::new(""))).unwrap(),
        Path::new("/")
    );
    assert_eq!(
        host::expanduser_with_home(Path::new("~/x"), Some(OsStr::new(""))).unwrap(),
        Path::new("/x")
    );
    let refusal = "Could not determine home directory.";
    let missing = Path::new("~agent-memory-no-such-user-9d8d035a01234/x");
    assert_eq!(
        host::expanduser_with_home(missing, home)
            .unwrap_err()
            .message(),
        refusal
    );
    assert_eq!(
        host::expanduser_with_home(Path::new("~/x"), Some(OsStr::new("~elsewhere")))
            .unwrap_err()
            .message(),
        refusal
    );
    // `root` has a passwd entry on every supported platform.
    let root = host::expanduser_with_home(Path::new("~root/x"), home).unwrap();
    assert!(root.is_absolute());
    assert!(root.ends_with("x"));
    assert!(!root.starts_with("~root"));
}
