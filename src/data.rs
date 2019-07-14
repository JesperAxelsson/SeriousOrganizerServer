use serious_organizer_lib::lens::{SortColumn, SortOrder};
use serde::{Deserialize, Deserializer, Serialize};
use num_derive::{FromPrimitive, ToPrimitive};


#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive)]
#[repr(u16)]
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
    FilterLabel = 16,
    AddLocation = 17,
    RemoveLocation = 18,
    GetLocations = 19,
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
    AddDirLabels(Vec<u32>, Vec<u32>),
    FilterLabel(u32, u8),

    AddLocation(String, String),
    RemoveLocation(u32),
    GetLocations,
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
