// Module defining operations with local and/or remote statefiles

use std::fs::File;
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write};
use std::time::SystemTime;
use resource::{ResHash};
use {MacrotisConfig, MacrotisStateConfig};
use s3;

// What is a state?  We just don't know.
#[derive(Serialize, Deserialize, Debug)]
pub struct MacrotisState {
    pub version: u32,
    pub appversion: String,
    pub serial: u64,
    pub records: ResHash
}

impl std::fmt::Display for MacrotisState {
	// Pretty print metadata about the state
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "Version {} (Macrotis v{}), Serial {}, {} resources",
				self.version, self.appversion, self.serial, self.records.0.len())
    }
}

// Struct methods
impl MacrotisState {
    // Make an 'empty' State struct pre-populated with the current app version
    // and current epoch time.
    pub fn new_empty() -> MacrotisState {
        let app_ver = env!("CARGO_PKG_VERSION");
        let rh = ResHash(HashMap::new());
        let right_now = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => panic!("Can't get time since epoch?!")
        };
        MacrotisState {
            version: 1,
            appversion: app_ver.to_string(),
            serial: right_now,
            records: rh
        }
    }
}

// Genericized state loading function; takes a MacrotisConfig struct and calls
// the more specific loader based on its contents.  Returns None on any errors,
// passes along MacrotisState on success.
pub fn load_state(config: &MacrotisConfig) -> Option<MacrotisState> {
    let stateconf = &config.statefile;

    // Check value of backend and ensure additional optional config settings
    // are present.
    match stateconf.backend.as_str() {
        "local" => {
            let fname = match &stateconf.filename {
                Some(x) => x,
                None => {
                    println!("Statefile backend set to 'local' but filename unset");
                    return None;
                }
            };
            load_local_state(&fname)
        },
        "s3" => {
            if check_bucket_params(&stateconf) {
                s3::fetch_state_file(&stateconf)
            } else {
                // Check_bucket_params will print what's missing; we can
                // just return None here.
                return None;
            }
        }
        _ => {
            println!("Unknown backend: {}", &stateconf.backend);
            return None;
        }
    }
}

// Attempt to load state from a local file.  Returns None if unable to load,
// MacrotisState with empty RecordHash if file does not exist.
pub fn load_local_state(fname: &str) -> Option<MacrotisState> {
    // Attempt to open and read file
    let f = match File::open(fname) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening statefile {}: {}", fname, e);
            return None;
        }
    };
    let reader = BufReader::new(f);
    let state: MacrotisState = match serde_json::from_reader(reader) {
        Ok(x) => x,
        Err(e) => {
            println!("Error parsing statefile JSON: {}", e);
            return None;
        }
    };
    Some(state)
}

// Genericized state saving function, operates same as load_state.  Returns
// true on success, false on failure.
pub fn save_state(config: &MacrotisConfig, recs: ResHash) -> bool {
    // Make an empty macrotis state and replace its innards with the received
    // RecordHash and serial, then turn it into a string of JSON with Serde
    let mut state = MacrotisState::new_empty();
    state.records = recs;

    let outstring = match serde_json::to_string_pretty(&state) {
        Ok(x) => x,
        Err(e) => {
            println!("Error serializing state to JSON: {}", e);
            return false;
        }
    };

    let stateconf = &config.statefile;
    match stateconf.backend.as_str() {
        "local" => {
            match save_local_state("foo", &outstring) {
                Ok(_) => true,
                Err(e) => {
                    println!("Error: {}", e);
                    false
                }
            }
        },
        "s3" => {
            match s3::put_state_file(&stateconf, &outstring) {
                Ok(_) => true,
                Err(e) => {
                    println!("Error: {}", e);
                    false
                }
            }
        },
        _ => {
            println!("Unknown backend: {}", &stateconf.backend);
            false
        }
    }
}

// Attempt to save state to a local file.
pub fn save_local_state(fname: &str, state: &str) -> Result<bool, String> {
    let f = match File::create(fname) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening state output file {}: {}", fname, e);
            return Err(e.to_string());
        }
    };
    let mut ofile_writer = BufWriter::new(f);
    match ofile_writer.write_all(state.as_bytes()) {
        Ok(_) => Ok(true),
        Err(e) => {
            println!("Error writing statefile {}: {}", fname, e);
            Err(e.to_string())
        }
    }
}

// Make sure all the necessary S3 bucket params are set in the state config.
pub fn check_bucket_params(conf: &MacrotisStateConfig) -> bool {
    let mut retval = true;

    // Check bucket name is present
    match &conf.bucket {
        Some(_) => { },
        None => {
            println!("No bucket name defined in state config");
            retval = false;
        }
    };

    // Check key is present
    match &conf.key {
        Some(_) => { },
        None => {
            println!("No bucket key defined in state config");
            retval = false;
        }
    };

    // These aren't critical; we can just fall back to defaults. But should
    // warn on them anyway.
    match &conf.region {
        Some(_) => { },
        None => {
            println!("No region defined in state config; will use default");
        }
    };

    match &conf.role_arn {
        Some(_) => {
            match &conf.session_name {
                Some(_) => { },
                None => {
                    println!("No session_name defined in state config; will assume role with 'default' session name");
                }
            };
        },
        None => {
            println!("No role_arn defined in state config; will not assume role for S3 operations");
        }
    };
    retval
}
