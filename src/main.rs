#![allow(unused_imports)]

extern crate rmp_serde as rmps;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serious_organizer_lib;
#[cfg(windows)]
extern crate winapi;
extern crate time;
use time::PreciseTime;

use std::ptr::{null, null_mut};
//use std::time::{Duration, Instant};

use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX,
                          PIPE_READMODE_MESSAGE,
                          PIPE_TYPE_MESSAGE};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::shared::minwindef::{DWORD, FALSE, LPVOID, LPCVOID, TRUE};
use winapi::um::fileapi::{ReadFile, WriteFile};
use serious_organizer_lib::dir_search;


use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};

mod data;
mod wstring;

use data::Test;
use wstring::to_wstring;

const BUFFER_SIZE: u32 = 1024;



fn main() {
    println!("Hello, world!");


//    let mut test = Test {
////        id: String::from("Hello"),
////        thing: 21,
////    };
////
////    let start = PreciseTime::now();
////
////    let mut out_buf = Vec::new();
////    let mut i = 0;
////    while i < 1_000_000 {
////
////        test.serialize(&mut Serializer::new(&mut out_buf)).expect("Failed to serialize");
////
////        let mut de = Deserializer::new(&out_buf[0..out_buf.len()]);
////        test = Deserialize::deserialize(&mut de).expect("Failed to deserialize");
////
////
////        i = i+1;
////    }
////    let end = PreciseTime::now();
////
////    println!("Took: {:?} ms", start.to(end).num_milliseconds());

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
                let start = PreciseTime::now();

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

                    let mut de = Deserializer::new(&buf[0..(dw_read as usize)]);
                    let test: Test = Deserialize::deserialize(&mut de).expect("Failed to deserialize");
                    println!("Data: {:?}", test);

                    let mut out_buf = Vec::new();
                    test.serialize(&mut Serializer::new(&mut out_buf)).expect("Failed to serialize");

                    WriteFile (h_pipe,
                               out_buf.as_ptr() as LPCVOID,
                               (out_buf.len()) as u32,
                               &mut dw_read,
                               null_mut());

                    let end = PreciseTime::now();
                    println!("Took: {:?} ms", start.to(end).num_milliseconds());

                }
            } else {
                DisconnectNamedPipe(h_pipe);
            }
        }
    }

    println!("Farewell, cruel world!");
}

