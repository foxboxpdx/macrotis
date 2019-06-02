#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate rusoto_core;
extern crate rusoto_route53;
extern crate rusoto_sts;
extern crate rusoto_s3;

use std::collections::HashMap;

// Sub-modules for parsing tinydns and interacting with AWS
pub mod tinydns;
pub mod r53;
pub mod s3;
pub mod state;

// Define a struct for holding configuration metadata
#[derive(Deserialize, Debug)]
pub struct MacrotisConfig {
    pub provider: MacrotisProviderConfig,
    pub statefile: MacrotisStateConfig,
    pub zones: Vec<R53Zone>
}

// Define a struct for holding provider configuration metadata
// If assume_role is true, role_arn needs to be populated
// Region is optional as well
#[derive(Serialize, Deserialize, Debug)]
pub struct MacrotisProviderConfig {
    pub name: String,
    pub region: Option<String>,
    pub assume_role: bool,
    pub role_arn: Option<String>,
    pub session_name: Option<String>
}

// Define a struct for holding State information - more or less just a remote
// copy of a parsed tinydns file.
#[derive(Serialize, Deserialize, Debug)]
pub struct MacrotisState {
    pub version: u32,
    pub appversion: String,
    pub serial: u64,
    pub records: RecordHash
}

// Define a struct for holding State configuration metadata
// If backend=local, only filename need be populated.  If backend=s3,
// everything else should be populated.
#[derive(Serialize, Deserialize, Debug)]
pub struct MacrotisStateConfig {
    pub backend: String,
    pub filename: Option<String>,
    pub bucket: Option<String>,
    pub key: Option<String>,
    pub region: Option<String>,
    pub role_arn: Option<String>,
    pub session_name: Option<String>,
    pub encrypt: Option<bool>,
    pub dynamodb_table: Option<String>
}

// Helper struct for R53 Zone data
#[derive(Deserialize, Debug)]
pub struct R53Zone {
    pub name: String,
    pub domain: String,
    pub id: String
}

// Define a struct defining a DNS record
// Really just the bare minimum required fields for a R53 ResourceRecord
#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct MacrotisRecord {
    pub zone_id: String,
    pub name: String,
    pub rtype: String,
    pub records: Vec<String>,
    pub ttl: i64
}

// This awkward type keeps showing up so I'm making it a 'newtype' struct.
#[derive(Serialize, Deserialize, Debug)]
pub struct RecordHash(pub HashMap<String, MacrotisRecord>);

// Equality
impl Eq for MacrotisRecord { }

impl PartialEq for MacrotisRecord {
    fn eq(&self, other: &Self) -> bool {
        let mut my_records = self.records.clone();
        my_records.sort();
        let mut other_records = other.records.clone();
        other_records.sort();
        self.zone_id == other.zone_id &&
        self.name    == other.name &&
        self.rtype   == other.rtype &&
        my_records   == other_records &&
        self.ttl     == other.ttl
    }
}

// And why not implement display?
impl std::fmt::Display for MacrotisRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}\t{}\tIN\t{}\t{:?}", self.name, self.ttl, self.rtype, self.records)
    }
}

impl MacrotisRecord {
    // Merge the records vectors of this and another struct
    // Return false if the record types are mismatched or there's
    // any other sorts of issues with the merge
    pub fn merge(&mut self, other: &Self) -> bool {
        if self.rtype != other.rtype {
            return false;
        }
        let mut newvec = other.records.clone();
        newvec.append(&mut self.records.clone());
        self.records = newvec;
        true
    }
}

// Shortcut for generating a new, empty MacrotisState, using the current version
// of the crate as 'appversion', a serial of 1, and an empty RecordHash.
impl MacrotisState {
    pub fn new_empty() -> MacrotisState {
        let app_ver = env!("CARGO_PKG_VERSION");
        let rh = RecordHash(HashMap::new());
        MacrotisState {
            version: 1,
            appversion: app_ver.to_string(),
            serial: 1,
            records: rh
        }
    }
}
