use serious_organizer_lib::lens::{SortColumn, SortOrder};
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
    Sort = 10,
    LabelAdd = 11,
    LabelRemove = 12,
    LabelsGet = 13,
    GetDirLabels = 14,
    AddDirLabels = 15,
}

#[derive(Debug)]
pub enum Request {
    DirCount,
    DirRequest(u32),
    FileRequest(u32, u32),
    DirFileCount(u32),
    ChangeSearchText(String),
    Reload,
    DeletePath(String),
    Sort(SortColumn, SortOrder),
    LabelAdd(String),
    LabelRemove(u32),
    LabelsGet,
    GetDirLabels(u32),
    AddDirLabels(u32, Vec<u32>)
}


#[derive(Serialize, Debug)]
pub struct DirEntryResponse {
    pub id: i32,
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
