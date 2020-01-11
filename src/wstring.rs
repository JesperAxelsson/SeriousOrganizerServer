#![allow(dead_code)]

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;

pub fn to_wstring(str: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(str).encode_wide().chain(once(0)).collect();

    if wide.capacity() != wide.len() {
        wide.shrink_to_fit();
    }

    wide
}

pub fn to_wnocstring(str: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(str).encode_wide().collect();

    if wide.capacity() != wide.len() {
        wide.shrink_to_fit();
    }

    wide
}

pub fn to_string(str: &[u16]) -> String {
    use std;
    let vec = str.split(|c| *c == 0).next();
    if !vec.is_none() {
        std::char::decode_utf16(vec.unwrap().iter().cloned())
            .map(|r| r.unwrap())
            .collect()
    } else {
        String::new()
    }
}
