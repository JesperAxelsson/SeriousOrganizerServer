#![deny(bare_trait_objects)]
#![allow(unused_imports)]

extern crate rmp_serde as rmps;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serious_organizer_lib;
extern crate time;
#[cfg(windows)]
extern crate winapi;
extern crate byteorder;
extern crate num;

use time::PreciseTime;

use std::ptr::{null, null_mut};
//use std::time::{Duration, Instant};

use winapi::shared::minwindef::{DWORD, FALSE, LPCVOID, LPVOID, TRUE};
use winapi::shared::ntdef::HANDLE;
use winapi::um::fileapi::{ReadFile, WriteFile};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE};

use serious_organizer_lib::{dir_search, lens, store};
use serious_organizer_lib::lens::{SortColumn, SortOrder};

use rmps::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

mod data;
mod wstring;

use data::{Request, RequestType};
use wstring::to_wstring;

const BUFFER_SIZE: u32 = 1024;

fn main() {
    println!("Hello, world!");

    let pipe_name = to_wstring("\\\\.\\pipe\\dude");
    let mut lens = lens::Lens::new();
    update_lens(&mut lens);

    unsafe {
        let h_pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            1,           // Max instances
            BUFFER_SIZE, // Out buffer
            BUFFER_SIZE, // In buffer
            0,           // default timeout
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

                        let req = parse_request(&buf);
                        let _sent = handle_request(h_pipe, req, &mut lens);

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
        success = WriteFile(
            pipe_handle,
            buf.as_ptr() as LPCVOID,
            (buf.len()) as u32,
            &mut dw_write,
            null_mut(),
        );
    }

    if success == FALSE {
        println!("Thingie closed during write?");
    }

    if success == TRUE && dw_write != buf.len() as u32 {
        panic!("Write less then buffer!");
    }

    dw_write as usize
}

fn parse_request(buf: &[u8]) -> Request {
    use std::io::Cursor;
    use std::mem::transmute;
    use byteorder::{ReadBytesExt, LittleEndian};

//    println!("Parsing Request");

    let request_type = unsafe { transmute(buf[0]) };
    let slice = &buf[1..];

    let mut rdr = Cursor::new(slice);

    match request_type {
        RequestType::ReloadStore => Request::Reload,
        RequestType::DirCount => Request::DirCount,
        RequestType::DirRequest => {
            let n1 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize DirRequest");
            Request::DirRequest(n1)
        }

        RequestType::DirFileCount => {
            let n1 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize DirFileCount");
            Request::DirFileCount(n1)
        }
        RequestType::FileRequest => {
            let n1 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize FileRequest start");
            let n2 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize FileRequest end");
            Request::FileRequest(n1, n2)
        }
        RequestType::ChangeSearchText => {
            let mut de = Deserializer::new(slice);
            let new_string =
                Deserialize::deserialize(&mut de).expect("Failed to deserialize ChangeSearchText");
            Request::ChangeSearchText(new_string)
        }
        RequestType::Sort => {
            let sort_column: u32 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize sort_column");
            let sort_order: u32 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize sort_order");

            Request::Sort(
                num::FromPrimitive::from_u32(sort_column).expect("Failed to parse sort_column"),
                num::FromPrimitive::from_u32(sort_order).expect("Failed to parse sort_order"))
        }
        _ => panic!("Unsupported request! {:?}", request_type),
    }
}

fn from_u32(number: u32) -> [u8; 4] {
    unsafe { std::mem::transmute(number) }
}

/***
    Request file:
    tag: u8
    ix: u32
    <tag><ix>
*/

fn handle_request(pipe_handle: HANDLE, req: Request, mut lens: &mut lens::Lens) -> usize {
    use data::*;

    //    println!("Handling Request");

    match req {
        Request::DirRequest(ix) => {
            //            println!("DirRequest: {:?}", ix);
            let mut out_buf = Vec::new();

            if let Some(dir) = lens.get_dir_entry(ix as usize) {
                //                let ref dir = lens.get_dir_entry(ix).expect("Ix_list index were invalid!");
                let dir_response = DirEntryResponse {
                    name: dir.name.clone(),
                    path: dir.path.clone(),
                    size: dir.size as u64,
                };
                dir_response
                    .serialize(&mut Serializer::new(&mut out_buf))
                    .expect("Failed to serialize DirRequest");
                send_response(pipe_handle, &out_buf)
            } else {
                out_buf.push(0xc0);
                send_response(pipe_handle, &out_buf)
            }
        }
        Request::FileRequest(dir_ix, file_ix) => {
            //            println!("DirRequest: {:?}", ix);
            println!("FileRequest dir: {} file: {}", dir_ix, file_ix);
            let mut out_buf = Vec::new();

            if let Some(file) = lens.get_file_entry(dir_ix as usize, file_ix as usize) {
                //                let ref dir = lens.get_dir_entry(ix).expect("Ix_list index were invalid!");
                let file_response = FileEntryResponse {
                    name: file.name.clone(),
                    path: file.path.clone(),
                    size: file.size as u64,
                };
                file_response
                    .serialize(&mut Serializer::new(&mut out_buf))
                    .expect("Failed to serialize FileRequest");
                send_response(pipe_handle, &out_buf)
            } else {
                out_buf.push(0xc0);
                send_response(pipe_handle, &out_buf)
            }
        }
        Request::ChangeSearchText(new_search_text) => {
            lens.update_search_text(&new_search_text);
            send_response(pipe_handle, &from_u32(lens.ix_list.len() as u32))
        }
        Request::DirCount => {
            println!("DirCount {}", lens.get_dir_count() as u32);
            send_response(pipe_handle, &from_u32(lens.get_dir_count() as u32))
        }
        Request::DirFileCount(ix) => {
            let file_count = lens
                .get_file_count(ix as usize)
                .expect(&format!("Invalid index {} during file count", ix))
                as u32;
            println!("FileCount {}", file_count);
            send_response(pipe_handle, &from_u32(file_count))
        }
        Request::Reload => {
            update_lens(&mut lens);
            let mut out_buf = Vec::new();
            out_buf.push(0);
            send_response(pipe_handle, &out_buf)
        }
        Request::DeletePath(_path) => 0,
    }
}

fn update_lens(lens: &mut lens::Lens) {
    let mut paths = Vec::new();
    paths.push(String::from("C:\\temp"));
    //    paths.push(String::from("J:\\temp"));
    //    paths.push(String::from("I:\\temp"));

    let mut dir_s = dir_search::get_all_data(paths);

    lens.update_data(&mut dir_s);
//    lens.order_by( SortColumn::Size, SortOrder::Desc);
}
