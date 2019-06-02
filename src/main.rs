extern crate macrotis;
#[macro_use] extern crate clap;

use macrotis::r53;
use macrotis::state;
use macrotis::{MacrotisConfig, RecordHash};
use macrotis::tinydns::{converter, parser};
use std::collections::HashMap;
//use macrotis::MacrotisRecord;
use std::env;
use std::fs::{File, metadata};
use std::path::Path;
use std::io::{BufReader, Read};
use clap::App;

// Main - Use Clap to build CLI, check options, etc.
fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).version(clap::crate_version!()).get_matches();

    // Safe to simply unwrap this value since it's marked as 'required'
    let input = matches.value_of("input").unwrap();

    // If no config file was specified, default to 'macrotis.conf'
    let conffile = matches.value_of("config").unwrap_or("macrotis.conf");

    // Attempt to load the config file, exit on failure
    let config = match load_config(conffile) {
        Some(x) => x,
        None => {
            println!("Error loading config file {}. Bailing out.", conffile);
            std::process::exit(1);
        }
    };

    // Make sure the AWS env keys are set
    match env::var("AWS_ACCESS_KEY_ID") {
        Ok(_) => {},
        Err(_) => {
            println!("AWS_ACCESS_KEY_ID unset, bailing out.");
            std::process::exit(1);
        }
    };
    match env::var("AWS_SECRET_ACCESS_KEY") {
        Ok(_) => {},
        Err(_) => {
            println!("AWS_SECRET_ACCESS_KEY unset, bailing out.");
            std::process::exit(1);
        }
    };

    // We have 3 primary collections of DNS records:
    // local: loaded from input file(s)
    // state: loaded from local or remote statefile
    // remote: downloaded from remote provider

    // Load and parse input file(s) to populate 'local'
    let mut local_recs = match load_local(&input, &config) {
        Some(x) => x,
        None => {
            println!("Error processing input file(s), bailing out.");
            std::process::exit(1);
        }
    };

    // If the -l (lint) flag was provided, stop here.
    if matches.is_present("lint") {
        let l = local_recs.0.len();
        println!("No errors detected; processed {} records", l); 
        std::process::exit(0);
    }

    // Load and parse statefile to populate 'state' - Note that state could
    // be empty if this is the first run!
    let state = match state::load_state(&config) {
        Some(x) => x,
        None => {
            println!("Error processing statefile, bailing out.");
            std::process::exit(1);
        }
    };
    let mut state_recs = state.records;
    let state_serial = state.serial;

    // NOTE remove me
    std::process::exit(0);

    // Load and parse remote provider zones to populate 'remote' - Note that
    // these could also be empty!
    let mut remote_recs = match load_remote(&config) {
        Some(x) => x,
        None => {
            println!("Error downloading remote records, bailing out.");
            std::process::exit(1);
        }
    };

    // Compare statefile records with remote records to ensure state accurately
    // reflects the 'source of truth'
    compare_state_and_remote(&mut state_recs, &remote_recs);

    // Compare local records with updated statefile records to see what changes
    // need to be sent to remote
    // Order of the tuple: New, Update, Delete
    let change_tuple = compare_local_and_state(&local_recs, &state_recs);

    // Compare the 'new' change set to the remote records, since it contains
    // records the statefile is unaware of but which might already exist
    // remotely.
    compare_new_and_remote(&change_tuple.0, &remote_recs);

    // Finally, send the changes up to the remote provider
    match push_remote(&change_tuple.0, &change_tuple.1, &change_tuple.2) {
        Ok(_) => { },
        Err(e) => {
            println!("Error sending changesets to remote: {}", e);
            std::process::exit(1);
        }
    };
    
    // Make the current local into the new state and write the new statefile
    state::save_state(&config, local_recs, state_serial);

    // Aaaand done.
    println!("Operation completed.");
}

// Load in a config file and deserialize it into a MacrotisConfig struct
fn load_config(fname: &str) -> Option<MacrotisConfig> {
    // Attempt to open and read file
    let f = match File::open(fname) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening file {}: {}", fname, e);
            return None;
        }
    };
    let reader = BufReader::new(f);

    // Deserialize
    let retval: MacrotisConfig = match serde_json::from_reader(reader) {
        Ok(x) => x,
        Err(e) => {
            println!("Error parsing config JSON: {}", e);
            return None;
        }
    };

    Some(retval)
}

// Load and parse input file(s)
// config is needed for TinyDNSRecord::find_zone_id
fn load_local(fname: &str, config: &MacrotisConfig) -> Option<RecordHash> {
    // A flag in case of errors; try to recover from as many as possible without
    // completely bailing out.
    let mut error_flag = false;

    // Check if input is a dir or a file using std::fs::metadata
    // call .is_dir() or .is_file() for an appropriate bool
    let meta = match metadata(&fname) {
        Ok(x) => x,
        Err(e) => {
            println!("Error reading {}: {}", fname, e);
            std::process::exit(1);
        }
    };

    // Call tinydns::parser::from_file either once (is_file) or in a loop
    // (is_dir).
    let mut retval = RecordHash { 0: HashMap::new() };
    if meta.is_file() {
        println!("Processing {}", &fname);
        let tdns_records = match parser::from_file(&fname) {
            Some(x) => x,
            None => {
                println!("Error processing input file {}", fname);
                return None;
            }
        };
        println!("Converting TinyDNS records...");
        let converted = match converter::tiny_to_macrotis(tdns_records, &config.zones) {
            Some(x) => x,
            None => {
                println!("Error converting tinydns to macrotis");
                return None;
            }
        };
        retval = RecordHash { 0: converted };
    } else {
        // Get a list of *.tiny files in the directory and call the tinydns
        // functions as necessary.
        // This is kinda gross???
        let mut tdns_vec = Vec::new();
        let path = Path::new(&fname);
        if let Ok(dir_iter) = std::fs::read_dir(&path) {
            for entry in dir_iter {
                if let Ok(f) = entry {
                    let fpath = f.path();
                    if fpath.is_dir() {
                        continue;
                    }
                    let pathstring = match fpath.to_str() {
                        Some(x) => x,
                        None => {
                            println!("Error getting path string for {:?}", fpath);
                            error_flag = true;
                            continue;
                        }
                    };
                    if let Some(ext) = fpath.extension() {
                        if ext == "tiny" {
                            println!("Processing {}...", &pathstring);
                            let mut recs = match parser::from_file(&pathstring) {
                                Some(mut x) => x,
                                None => {
                                    println!("Error processing {}", pathstring);
                                    error_flag = true;
                                    continue;
                                }
                            };
                            tdns_vec.append(&mut recs);
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                } else {
                    println!("Error getting entry from iterator");
                    error_flag = true;
                    continue;
                }
            } // End of loop, convert the big vec
            println!("Converting TinyDNS records...");
            let converted = match converter::tiny_to_macrotis(tdns_vec, &config.zones) {
                Some(x) => x,
                None => {
                    println!("Error converting tinydns to macrotis");
                    return None;
                }
            };
            retval = RecordHash { 0: converted };
        } else {
            println!("Error getting iterator for {}", path.display());
            return None;
        }

    }
    if error_flag {
        None
    } else {
        Some(retval)
    }
}

// Load and parse remote records
fn load_remote(config: &MacrotisConfig) -> Option<RecordHash> {
    let prov = &config.provider;
    let z = &config.zones[0];
    match r53::bulk_fetch(prov, &z.id) {
        Some(x) => {
            println!("Got {} records", x.len());
        },
        None => {
            println!("Didn't get any records!");
        }
    };
    None
}

// Compare and update the statefile records to match remote
fn compare_state_and_remote(state: &mut RecordHash, remote: &RecordHash) {
}

// Compare local records and statefile records; return a tuple of RecordHash
// structs representing New, Updated, and Deleted records
fn compare_local_and_state(local: &RecordHash, state: &RecordHash) 
                          -> (RecordHash, RecordHash, RecordHash) {
    (RecordHash { 0: HashMap::new() },
     RecordHash { 0: HashMap::new() }, 
     RecordHash { 0: HashMap::new() })
}

// Compare 'new' records and remote records
fn compare_new_and_remote(new: &RecordHash, remote: &RecordHash) {
}

// Push records up to remote
fn push_remote(new: &RecordHash, update: &RecordHash, del: &RecordHash) -> Result<(), String> {
    Ok(())
}

// Turn local records into new statefile and push/write it to wherever it lives
fn push_state(records: &RecordHash, config: &MacrotisConfig) {
}

