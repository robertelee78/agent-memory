use super::descriptor::{Descriptor, DirectoryChain};
use crate::error::{ContractError, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

pub fn inode_flags(descriptor: &Descriptor) -> io::Result<u32> {
    #[cfg(target_os = "macos")]
    { Ok(descriptor.native_stat()?.st_flags) }
    #[cfg(target_os = "linux")]
    {
        let request = libc::_IOR::<libc::c_long>(b'f' as u32, 1);
        let mut flags: libc::c_long = 0;
        if unsafe { libc::ioctl(descriptor.as_raw_fd(), request, &mut flags) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags as u32)
    }
}

fn list_raw(descriptor: &Descriptor, buffer: *mut libc::c_char, size: usize) -> isize {
    #[cfg(target_os = "macos")]
    { unsafe { libc::flistxattr(descriptor.as_raw_fd(), buffer, size, 0) } }
    #[cfg(target_os = "linux")]
    { unsafe { libc::flistxattr(descriptor.as_raw_fd(), buffer, size) } }
}

pub fn list_xattrs(descriptor: &Descriptor) -> io::Result<Vec<Vec<u8>>> {
    let count = list_raw(descriptor, std::ptr::null_mut(), 0);
    if count == -1 { return Err(io::Error::last_os_error()); }
    if count > 16384 { return Err(io::Error::new(io::ErrorKind::InvalidData, "Extended attribute names exceed their bound")); }
    if count == 0 { return Ok(Vec::new()); }
    let mut bytes = vec![0u8; count as usize];
    let actual = list_raw(descriptor, bytes.as_mut_ptr().cast(), bytes.len());
    if actual == -1 { return Err(io::Error::last_os_error()); }
    if actual != count || bytes.last() != Some(&0) { return Err(io::Error::new(io::ErrorKind::InvalidData, "Extended attribute names changed")); }
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for name in bytes[..bytes.len() - 1].split(|byte| *byte == 0) {
        if name.is_empty() || !seen.insert(name.to_vec()) { return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid duplicate extended attribute names")); }
        names.push(name.to_vec());
    }
    Ok(names)
}

fn get_raw(descriptor: &Descriptor, name: &CStr, buffer: *mut libc::c_void, size: usize) -> isize {
    #[cfg(target_os = "macos")]
    { unsafe { libc::fgetxattr(descriptor.as_raw_fd(), name.as_ptr(), buffer, size, 0, 0) } }
    #[cfg(target_os = "linux")]
    { unsafe { libc::fgetxattr(descriptor.as_raw_fd(), name.as_ptr(), buffer, size) } }
}

pub fn read_xattr(descriptor: &Descriptor, name: &[u8]) -> io::Result<Vec<u8>> {
    let name = CString::new(name).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in extended attribute name"))?;
    let count = get_raw(descriptor, &name, std::ptr::null_mut(), 0);
    if count == -1 { return Err(io::Error::last_os_error()); }
    if count > 16384 { return Err(io::Error::new(io::ErrorKind::InvalidData, "Extended attribute exceeds its bound")); }
    let mut value = vec![0u8; count as usize];
    let actual = get_raw(descriptor, &name, value.as_mut_ptr().cast(), value.len());
    if actual == -1 { return Err(io::Error::last_os_error()); }
    if actual != count { return Err(io::Error::new(io::ErrorKind::InvalidData, "Extended attribute changed")); }
    Ok(value)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(acl: *mut libc::c_void, entry: libc::c_int, result: *mut *mut libc::c_void) -> libc::c_int;
    fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
}

#[cfg(target_os = "macos")]
fn refuse_acl(descriptor: &Descriptor) -> io::Result<()> {
    let errno = unsafe { libc::__error() };
    unsafe { *errno = 0; }
    let acl = unsafe { acl_get_fd_np(descriptor.as_raw_fd(), 0x100) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        return if matches!(error.raw_os_error(), Some(libc::ENOENT) | Some(libc::ENODATA)) { Ok(()) } else { Err(error) };
    }
    struct Acl(*mut libc::c_void);
    impl Drop for Acl { fn drop(&mut self) { unsafe { acl_free(self.0); } } }
    let acl = Acl(acl);
    let mut entry = std::ptr::null_mut();
    unsafe { *errno = 0; }
    let result = unsafe { acl_get_entry(acl.0, 0, &mut entry) };
    if result == 0 || (result == -1 && !matches!(io::Error::last_os_error().raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOENT))) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Nonempty or unreadable ACL"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes { output.push(DIGITS[(byte >> 4) as usize] as char); output.push(DIGITS[(byte & 15) as usize] as char); }
    output
}

pub fn metadata_supported(path: &Path) -> Result<BTreeMap<String, String>> {
    let observe = || -> io::Result<BTreeMap<String, String>> {
        let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Metadata path has no parent"))?;
        let name: &OsStr = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Metadata path has no filename"))?;
        let parents = DirectoryChain::open(parent)?;
        let before = parents.directory().stat_at(name)?;
        let descriptor = parents.directory().open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        if descriptor.stat()? != before { return Err(io::Error::other("Metadata inode changed")); }
        let flags = inode_flags(&descriptor)?;
        #[cfg(target_os = "linux")]
        if flags & !(0x00080000 | 0x00001000) != 0 { return Err(io::Error::other("Unsupported inode flags")); }
        #[cfg(target_os = "macos")]
        if flags != 0 { return Err(io::Error::other("Unsupported inode flags")); }
        let names = list_xattrs(&descriptor)?;
        #[cfg(target_os = "macos")]
        let mut attributes = BTreeMap::new();
        #[cfg(target_os = "linux")]
        let attributes = BTreeMap::new();
        #[cfg(target_os = "linux")]
        if !names.is_empty() { return Err(io::Error::other("Unsupported extended attributes")); }
        #[cfg(target_os = "macos")]
        {
            for name in &names {
                if name != b"com.apple.provenance" { return Err(io::Error::other("Unsupported extended attributes")); }
                attributes.insert("com.apple.provenance".to_owned(), hex(&read_xattr(&descriptor, name)?));
            }
            refuse_acl(&descriptor)?;
        }
        if inode_flags(&descriptor)? != flags || descriptor.stat()? != before || parents.directory().stat_at(name)? != before || list_xattrs(&descriptor)? != names {
            return Err(io::Error::other("Metadata changed during observation"));
        }
        #[cfg(target_os = "macos")]
        {
            for (name, value) in &attributes {
                if hex(&read_xattr(&descriptor, name.as_bytes())?) != *value { return Err(io::Error::other("Metadata changed during observation")); }
            }
            refuse_acl(&descriptor)?;
        }
        parents.recheck()?;
        Ok(attributes)
    };
    observe().map_err(|_| ContractError::new("pending-metadata-preservation"))
}
