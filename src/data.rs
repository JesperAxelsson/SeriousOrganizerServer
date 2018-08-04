use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[repr(u8)]
pub enum RequestType {
    DirRequest = 1,
    FileRequest = 2,
    AddPath = 3,
    RemovePath = 4,
    ReloadStore = 5,
    ChangeSearchText = 6,
    DirCount = 7,
    DirFileCount = 8,
    DeletePath = 9,
}

#[derive(Serialize, Debug)]
pub struct DirEntryResponse {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Serialize, Debug)]
pub struct FileEntryResponse {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Deserialize, Debug)]
pub enum Request {
    DirCount,
    DirRequest(u32),
    FileRequest(u32, u32),
    DirFileCount(u32),
    ChangeSearchText(String),
    Reload,
    DeletePath(String),
}
