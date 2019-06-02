// Module defining operations with local and/or remote statefiles

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use {MacrotisState, MacrotisConfig, MacrotisStateConfig, RecordHash};
use s3;

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
pub fn save_state(config: &MacrotisConfig, recs: RecordHash, serial: u64) -> bool {
    // Make an empty macrotis state and replace its innards with the received
    // RecordHash and serial, then turn it into a string of JSON with Serde
    let mut state = MacrotisState::new_empty();
    state.records = recs;
    state.serial = serial;

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
