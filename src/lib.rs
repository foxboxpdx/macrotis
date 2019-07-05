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
pub mod resource;
pub mod s3;
pub mod state;
pub mod compare;

// Define a struct for holding configuration metadata
#[derive(Deserialize, Debug)]
pub struct MacrotisConfig {
    pub provider: MacrotisProviderConfig,
    pub statefile: MacrotisStateConfig,
    pub zones: Vec<Zone>
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
    pub tags: Option<HashMap<String, String>>,
    pub session_name: Option<String>,
}

// Helper struct for Zone data
#[derive(Deserialize, Debug)]
pub struct Zone {
    pub name: String,
    pub domain: String,
    pub id: String
}


