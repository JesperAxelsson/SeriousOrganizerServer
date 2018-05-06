//use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Test {
    pub id: String,
    pub thing: i32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, )]
#[repr(u8)]
pub enum RequestType {
    Test = 0,
    DirRequest = 1,
    FileRequest = 2,
    AddPath = 3,
    RemovePath = 4,
    ReloadStore = 5,
    ChangeSearchText = 6,
    DirCount = 7,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct DirEntryResponse {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
}
