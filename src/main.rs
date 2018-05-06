#![allow(unused_imports)]

#[macro_use(bson, doc)]
extern crate bson;
extern crate rmp_serde as rmps;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serious_organizer_lib;
#[cfg(windows)]
extern crate winapi;
extern crate time;
use time::PreciseTime;

use std::ptr::{null, null_mut};

use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX, PIPE_READMODE_MESSAGE, PIPE_TYPE_BYTE,
                          PIPE_TYPE_MESSAGE};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::shared::minwindef::{DWORD, FALSE, LPVOID, LPCVOID, TRUE};
use winapi::um::fileapi::{ReadFile, WriteFile};
use serious_organizer_lib::dir_search;
use std::mem::transmute;

const BUFFER_SIZE: u32 = 1024;

#[derive(Serialize, Deserialize, Debug)]
pub struct Test {
//    #[serde(rename = "_id")]  // Use MongoDB's special primary key field name when serializing
    pub id: String,
    pub thing: i32,
}

use std::time::{Duration, Instant};
fn main() {
    println!("Hello, world!");

//    let mut test = Test {
//        id: String::from("Hello"),
//        thing: 21,
//    };

//    let start = PreciseTime::now();
//
//    let mut i = 0;
//    while i < 1_000_000 {
//
//        let tt = bson::to_bson( &test).expect("Failed to encode bson");
//        let tt2 = bson::from_bson::< Test > (tt).expect("Failed to decode bson");
//
//        i = i+1;
//    }
//    let end = PreciseTime::now();

//    println!("Took: {:?} ms", start.to(end).num_milliseconds());

    let pipe_name = to_wstring("\\\\.\\pipe\\dude");

    unsafe {
        let h_pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            1,    // Max instances
            BUFFER_SIZE, // Out buffer
            BUFFER_SIZE, // In buffer
            0,    // default timeout
            null_mut(),
        );

        while h_pipe != INVALID_HANDLE_VALUE {
            let connected = ConnectNamedPipe(h_pipe, null_mut());
            if connected != FALSE {
                println!("Connected!");
                let mut buf = [0u8; BUFFER_SIZE as usize];
                let mut dw_read: DWORD = 0;
                while ReadFile(
                    h_pipe,
                    &mut buf as *mut _ as LPVOID,
                    ((buf.len()) - 1) as u32,
                    &mut dw_read,
                    null_mut(),
                ) != FALSE
                {

                    use std::io::Cursor;
//                    let s = String::from_utf8_lossy(&buf[0..(dw_read as usize)]);
                    let doc = bson::decode_document(&mut Cursor::new(&buf[0..(dw_read as usize)])).expect("Failed to decode document");

                    let tt2 = bson::from_bson::<Test>(bson::Bson::from(doc)).expect("Failed to decode bson");
                    println!("Data: {:?}", tt2);
//                    println!("Data: {:?}", s);
                    //                    println!("Data: {:?}", to_string(&buf[0..(dw_read as usize)]));

                    let mut vec: Vec<u8> = Vec::new();
                   if let bson::Bson::Document(doc) = bson::to_bson(&tt2).expect("Failed to enocode bson") {
//                       let mut bb = vec.

                       bson::encode_document(&mut vec, &doc).unwrap();
                        WriteFile (h_pipe, &mut vec.as_ptr() as *mut _ as LPCVOID,
                                   (vec.len()) as u32,
                                   &mut dw_read,
                                      null_mut());
                   }

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
        std::char::decode_utf16(vec.unwrap().iter().cloned())
            .map(|r| r.unwrap())
            .collect()
    } else {
        String::new()
    }
}
