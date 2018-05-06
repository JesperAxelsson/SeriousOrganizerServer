//use serde::{Deserialize, Serialize};



#[derive(Serialize, Deserialize, Debug)]
pub struct Test {
    //    #[serde(rename = "_id")]  // Use MongoDB's special primary key field name when serializing
    pub id: String,
    pub thing: i32,
}
