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
use winapi::shared::ntdef::{HANDLE};

use serious_organizer_lib::{dir_search, lens};

use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};

mod data;
mod wstring;

use data::{Test, RequestType};
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

    let mut paths= vec![String::from("C:\\home\\bin\\SysInternal")];
    paths.push(String::from("C:\\temp"));
//    paths.push(String::from("D:\\temp"));
    let mut dir_s = dir_search::get_all_data(paths);
    let mut lens = lens::Lens::new();
    lens.update_data(&mut dir_s);

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

                    let _start = PreciseTime::now();

                    let (offset, req) = parse_request(&buf);
                    let _sent = handle_request(h_pipe, &buf[offset..(dw_read as usize)], req, &mut lens);

                    let _end = PreciseTime::now();
//                    println!("{} bytes took {:?} ms", sent, start.to(end).num_milliseconds());

                }
            } else {
                DisconnectNamedPipe(h_pipe);
            }
        }
    }

    println!("Farewell, cruel world!");
}

fn send_response(pipe_handle: HANDLE, buf: &[u8]) -> usize {
    let mut dw_write: DWORD = 0;
    let success;
    unsafe {
        success = WriteFile(pipe_handle,
                  buf.as_ptr() as LPCVOID,
                  (buf.len()) as u32,
                  &mut dw_write,
                  null_mut());

    }

    if success == FALSE {
        println!("Thingie closed during write?");
    }

    if success == TRUE && dw_write != buf.len() as u32 {
        panic!("Write less then buffer!");
    }

     dw_write as usize
}


pub enum Req {
    DirCount,
    DirRequest(u32),
    ChangeSearchText,
    Reload,
    Test,
}

fn parse_request(buf: &[u8]) -> (usize, Req) {
    use std::mem::transmute;
//    println!("Parsing Request");
    let request_type = unsafe{ transmute(buf[0])};
    match request_type {
        RequestType::Test => (1, Req::Test),
        RequestType::DirRequest => (5, Req::DirRequest(get_u32(&buf[1..5]))),
        RequestType::ReloadStore => (1, Req::Reload),
        RequestType::DirCount => (1, Req::DirCount),
        RequestType::ChangeSearchText => (1, Req::ChangeSearchText),
        _ => (0, Req::Test),
    }
}

fn get_u32(buf: &[u8]) -> u32 {
    let mut tmp_buf: [u8; 4] = [0,0,0,0];
    if buf.len() != 4 {
        panic!("Has to before is: {}", buf.len());
    }
    tmp_buf.copy_from_slice(buf);
    let number = unsafe { std::mem::transmute::<[u8; 4], u32>(tmp_buf )  };
//    (4, number)
    number
}

fn from_u32(number: u32) -> [u8; 4] {
    unsafe{ std::mem::transmute(number)}
}

/***
    Request file:
    tag: u8
    ix: u32
    <tag><ix>
*/

fn handle_request(pipe_handle: HANDLE, buf: &[u8], req: Req, lens: &mut lens::Lens) -> usize {
    use data::*;

//    println!("Handling Request");

    match req {
        Req::DirRequest(ix) => {
//            println!("DirRequest: {:?}", ix);
            let mut out_buf = Vec::new();

            if let Some(ix) = lens.convert_ix(ix as usize) {
                let ref dir = lens.get_dir_entry(ix).expect("Ix_list index were invalid!");
                let dir_response = DirEntryResponse {
                    name: dir.name.clone(),
                    path: dir.path.clone(),
                    size: dir.size,
                };
                dir_response.serialize(&mut Serializer::new(&mut out_buf)).expect("Failed to serialize DirRequest");
                send_response(pipe_handle, &out_buf)
            } else {
                out_buf.push(0xc0);
                send_response(pipe_handle, &out_buf)
            }
        },
        Req::ChangeSearchText => {
            let mut de = Deserializer::new(buf);
            let new_search_text: String = Deserialize::deserialize(&mut de).expect("Failed to deserialize ChangeSearchText");
            lens.update_search_text(&new_search_text);
            send_response(pipe_handle, &from_u32(lens.ix_list.len() as u32))
        },
        Req::DirCount => {
            println!("DirCount {}", lens.ix_list.len() as u32);
            send_response(pipe_handle, &from_u32(lens.ix_list.len() as u32))
        },
        Req::Reload => {
            0
        },
        Req::Test => {
            let mut de = Deserializer::new(buf);
            let test: Test = Deserialize::deserialize(&mut de).expect("Failed to deserialize Test");
            let mut out_buf = Vec::new();
            test.serialize(&mut Serializer::new(&mut out_buf)).expect("Failed to serialize Test");
            println!("Data: {:?}", test);
            send_response(pipe_handle, &out_buf)
        }
    }
}
