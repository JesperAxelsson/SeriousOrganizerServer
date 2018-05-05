#![allow(unused_imports)]
#[cfg(windows)] extern crate winapi;

use std::ptr::{null, null_mut};

use winapi::um::namedpipeapi::{CreateNamedPipeW, ConnectNamedPipe, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE, PIPE_TYPE_MESSAGE, PIPE_READMODE_MESSAGE };
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::shared::minwindef::{FALSE, TRUE, LPVOID, DWORD};
use winapi::um::fileapi::{ReadFile};

fn main() {
    println!("Hello, world!");

    let pipe_name = to_wstring("\\\\.\\pipe\\dude");

    unsafe  {
        let h_pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            1, // Max instances
            1024, // Out buffer
            1024, // In buffer
            0, // default timeout
            null_mut());

        while h_pipe != INVALID_HANDLE_VALUE {
            let connected = ConnectNamedPipe(h_pipe, null_mut());
            if connected != FALSE {
                println!("Connected!");
                let mut buf = [0u8; 1024];
                let mut dw_read: DWORD = 0;
                while ReadFile(h_pipe, &mut buf as *mut _ as LPVOID, ((buf.len())-1) as u32, &mut dw_read, null_mut()) != FALSE {
                    let s = String::from_utf8_lossy(&buf[0..(dw_read as usize)]);

                    println!("Data: {:?}", s);
//                    println!("Data: {:?}", to_string(&buf[0..(dw_read as usize)]));
                }
            } else {
                DisconnectNamedPipe(h_pipe);
            }

        }
    }

    println!("Farewell, cruel world!");
}


use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::iter::once;


pub fn to_wstring(str: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(str).encode_wide().chain(once(0)).collect();

    if wide.capacity() != wide.len() {
        wide.shrink_to_fit();
    }

    wide
}

pub fn to_string(str: &[u16]) -> String {
    use std;
    let vec = str.split(|c| *c == 0).next();
    if !vec.is_none() {
        std::char::decode_utf16(vec.unwrap().iter().cloned()).map(|r| r.unwrap()).collect()
    } else {
        String::new()
    }
}
