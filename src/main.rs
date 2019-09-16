#![deny(bare_trait_objects)]
#![allow(unused_imports)]
#![allow(unused_variables)]

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
#[macro_use] 
extern crate log;

use time::PreciseTime;

use std::ptr::{null, null_mut};
//use std::time::{Duration, Instant};

use winapi::shared::minwindef::{DWORD, FALSE, LPCVOID, LPVOID, TRUE};
use winapi::shared::ntdef::HANDLE;
use winapi::um::fileapi::{ReadFile, WriteFile};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_READMODE_BYTE};

use serious_organizer_lib::{dir_search, lens, store};
use serious_organizer_lib::lens::{SortColumn, SortOrder};

use rmps::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

use simplelog::*;

pub mod data;
pub mod wstring;


use crate::data::*;
use crate::wstring::to_wstring;
use std::io::{Read, Error};
use std::io::Cursor;
use byteorder::{ReadBytesExt, LittleEndian};

const BUFFER_SIZE: u32 = 500 * 1024;

fn main() {
     CombinedLogger::init(
        vec![
            SimpleLogger::new(LevelFilter::Info, Config::default()),
            WriteLogger::new(LevelFilter::Info, Config::default(), std::fs::File::create("serious_server.log").expect("Failed to init logger")),
        ]
    ).unwrap();

    info!("Hello, world!");

    let pipe_name = to_wstring("\\\\.\\pipe\\dude");
    let mut lens = lens::Lens::new();
//    update_lens(&mut lens);

    unsafe {
        let h_pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_BYTE,
            1,           // Max instances
            BUFFER_SIZE, // Out buffer
            BUFFER_SIZE, // In buffer
            0,           // default timeout
            null_mut(),
        );

        while h_pipe != INVALID_HANDLE_VALUE {
            let connected = ConnectNamedPipe(h_pipe, null_mut());
            if connected != FALSE {
                debug!("Connected!");

                let mut buf = [0u8; BUFFER_SIZE as usize];
                let mut dw_read: DWORD = 0;

                while let Some(size) = read_size(h_pipe) {
                    if ReadFile(
                        h_pipe,
                        &mut buf as *mut _ as LPVOID,
                        size,
                        &mut dw_read,
                        null_mut(),
                    ) != FALSE {
                       trace!("Read data: {:?} as int: {:?}", dw_read, buf[0..(size as usize)].to_vec());

                        let _start = PreciseTime::now();

                        let req = parse_request(&buf);
                        let _sent = handle_request(h_pipe, req, &mut lens);

                        let _end = PreciseTime::now();

                       trace!("{} bytes took {:?} ms", _sent, _start.to(_end).num_milliseconds());
                    }
                }
            } else {
                DisconnectNamedPipe(h_pipe);
            }
        }
    }

    info!("Farewell, cruel world!");
}

unsafe fn read_size(pipe_handle: HANDLE) -> Option<u32> {
    let mut size_buf = [0u8; 4];
    let mut dw_read: DWORD = 0;

    if ReadFile(
        pipe_handle,
        &mut size_buf as *mut _ as LPVOID,
        size_buf.len() as u32,
        &mut dw_read,
        null_mut(),
    ) != FALSE {
        let size = to_u32(size_buf);
        return Some(size);
    } else {
        return None;
    }
}

fn send_response(pipe_handle: HANDLE, buf: &[u8]) -> usize {
    let mut dw_write: DWORD = 0;
    let success;
    let size_buf = from_u32(buf.len() as u32);

    unsafe {
        WriteFile(
            pipe_handle,
            size_buf.as_ptr() as LPCVOID,
            size_buf.len() as u32,
            &mut dw_write,
            null_mut(),
        );

        success = WriteFile(
            pipe_handle,
            buf.as_ptr() as LPCVOID,
            buf.len() as u32,
            &mut dw_write,
            null_mut(),
        );
    }

    if success == FALSE {
        warn!("Thingie closed during write?");
    }

    if success == TRUE && dw_write != buf.len() as u32 {
        error!("Write less then buffer!");
        panic!("Write less then buffer!");
    }

    dw_write as usize
}

fn parse_request(buf: &[u8]) -> Request {
    use std::mem::transmute;

    let slice = &buf[0..];
    let mut rdr = Cursor::new(slice);
    let request_type: RequestType = num::FromPrimitive::from_u16(rdr.read_u16::<LittleEndian>().unwrap()).expect("Failed to read request type");
    trace!("Got request: {:?}", request_type);

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
            let mut de = Deserializer::new(&slice[2..]);
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

        RequestType::LabelAdd => {
            let mut de = Deserializer::new(&slice[2..]);
            let new_string =
                Deserialize::deserialize(&mut de).expect("Failed to deserialize LabelAdd");
            Request::LabelAdd(new_string)
        }
        RequestType::LabelRemove => {
            let n1 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize LabelRemove");
            Request::LabelRemove(n1)
        }
        RequestType::LabelsGet => {
            Request::LabelsGet
        }

        RequestType::GetDirLabels => {
            let n1 = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize GetDirLabels");
            Request::GetDirLabels(n1)
        }
        RequestType::AddDirLabels => {
            let entries = read_list(&mut rdr).expect("AddDirLabels(): Failed to read entries list");
            let label_ids = read_list(&mut rdr).expect("AddDirLabels(): Failed to read labels list");

            Request::AddDirLabels(entries, label_ids)
        }
        RequestType::FilterLabel => {
            let label_id = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize FilterLabel label_id");
            let state = rdr.read_u8().expect("Failed to deserialize FilterLabel state");

            Request::FilterLabel(label_id, state)
        }

        RequestType::AddLocation => {
            let raw_s1 = read_byte_list(&mut rdr).unwrap();
            let raw_s2 = read_byte_list(&mut rdr).unwrap();

            let name_string = String::from_utf8(raw_s1).expect("Failed to deserialize AddLocation name_string");
            let path_string=  String::from_utf8(raw_s2).expect("Failed to deserialize AddLocation path_string");

            Request::AddLocation(name_string, path_string)
        }
        RequestType::RemoveLocation => {
            let location_id = rdr.read_u32::<LittleEndian>().expect("Failed to deserialize RemoveLocation location_id");

            Request::RemoveLocation(location_id)
        }
        RequestType::GetLocations => {
            Request::GetLocations
        }

        _ => panic!("Unsupported request! {:?}", request_type),
    }
}

fn from_u32(number: u32) -> [u8; 4] {
    unsafe { std::mem::transmute(number) }
}

fn to_u32(number_buf: [u8; 4]) -> u32 {
    unsafe { std::mem::transmute(number_buf) }
}

fn read_list(reader: &mut Cursor<&[u8]>) -> Result<Vec<u32>, std::io::Error> {
    let list_count = reader.read_u32::<LittleEndian>()?;

    let mut list = Vec::new();

    for _ in 0..list_count {
        let id = reader.read_u32::<LittleEndian>()?;
        list.push(id);
    }

    return Ok(list);
}

fn read_byte_list(reader: &mut Cursor<&[u8]>) -> Result<Vec<u8>, std::io::Error> {
    let list_count = reader.read_u32::<LittleEndian>()?;

    let mut list = Vec::new();

    for _ in 0..list_count {
        let id = reader.read_u8()?;
        list.push(id);
    }

    return Ok(list);
}

/***
    Request file:
    tag: u8
    ix: u32
    <tag><ix>
*/

fn handle_request(pipe_handle: HANDLE, req: Request, mut lens: &mut lens::Lens) -> usize {
    trace!("Handling Request");

    match req {
        Request::DirRequest(ix) => handle_dir_request(pipe_handle, &lens, ix),
        Request::FileRequest(dir_ix, file_ix) => handle_file_request(pipe_handle, &lens, dir_ix, file_ix),
        Request::ChangeSearchText(new_search_text) => {
            lens.update_search_text(&new_search_text);
            send_response(pipe_handle, &from_u32(lens.ix_list.len() as u32))
        }
        Request::DirCount => {
            debug!("DirCount {}", lens.get_dir_count() as u32);
            send_response(pipe_handle, &from_u32(lens.get_dir_count() as u32))
        }
        Request::DirFileCount(ix) => {
            let file_count = lens
                .get_file_count(ix as usize)
                .expect(&format!("Invalid index {} during file count", ix))
                as u32;
            debug!("FileCount {}", file_count);
            send_response(pipe_handle, &from_u32(file_count))
        }
        Request::Reload => {
            update_lens(&mut lens);
            let mut out_buf = Vec::new();
            out_buf.push(0);
            send_response(pipe_handle, &out_buf)
        }
        Request::DeletePath(_path) => 0,
        Request::Sort(col, order) => {
           debug!("SortRequest: {:?} {:?}", col, order);
            lens.order_by(col, order);
            let r: u32 = 1;
            send_response(pipe_handle, &from_u32(r))
        }

        Request::LabelAdd(name) => {
           debug!("LabelAdd: {:?}", name);
            lens.add_label(&name);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::LabelRemove(id) => {
            debug!("LabelRemove: {:?}", id);
            lens.remove_label(id);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::LabelsGet => {
           debug!("LabelsGet");
            handle_labels_request(pipe_handle, &lens)
        }

        Request::GetDirLabels(entry_id) => {
           debug!("GetDirLabels");
            handle_dir_labels_request(pipe_handle, entry_id, &lens)
        }
        Request::AddDirLabels(entries, label_ids) => {
           debug!("AddDirLabels() Got entry {:?} and labels {:?} ", entries.len(), label_ids.len());
            lens.set_entry_labels(entries, label_ids);
            send_response(pipe_handle, &from_u32(0))
        }

        Request::FilterLabel(label_id, state) => {
            match state {
                0 => lens.remove_label_filter(label_id),
                1 => lens.add_inlude_label(label_id),
                2 => lens.add_exclude_label(label_id),
                _ => panic!("Ermagad, this state is not supported!"),
            }

            send_response(pipe_handle, &from_u32(0))
        }

        Request::AddLocation(name, path) => {
            lens.add_location(&name, &path);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::RemoveLocation(location_id) => {
            lens.remove_location(location_id);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::GetLocations => handle_locations_request(pipe_handle, &lens)
        // add update?
    }
}



fn handle_dir_request(pipe_handle: HANDLE, lens: &lens::Lens, ix: u32) -> usize {
    use serious_organizer_lib::models::EntryId;

    let mut out_buf = Vec::new();

    if let Some(dir) = lens.get_dir_entry(ix as usize) {
        let EntryId(entry_id) = dir.id;
        let dir_response = DirEntryResponse {
            id: entry_id,
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


fn handle_file_request(pipe_handle: HANDLE, lens: &lens::Lens, dir_ix: u32, file_ix: u32) -> usize {
    trace!("FileRequest dir: {} file: {}", dir_ix, file_ix);
    let mut out_buf = Vec::new();

    if let Some(file) = lens.get_file_entry(dir_ix as usize, file_ix as usize) {
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


fn handle_labels_request(pipe_handle: HANDLE, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.get_labels().serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize labels request");
    trace!("handle_labels_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn handle_dir_labels_request(pipe_handle: HANDLE, entry_id: u32, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.entry_labels(entry_id).serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize label for entries");
    trace!("handle_labels_for_entry_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn handle_locations_request(pipe_handle: HANDLE, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.get_locations().serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize locations request");
    trace!("handle_locations_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn update_lens(lens: &mut lens::Lens) {
    let paths = lens.get_locations().iter().map(|e| (e.id, e.path.clone())).collect();
    let mut dir_s = dir_search::get_all_data(paths);

    lens.update_data(&mut dir_s);
}
