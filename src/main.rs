extern crate macrotis;
#[macro_use] extern crate clap;

use macrotis::r53;
use macrotis::state;
use macrotis::resource;
use macrotis::compare;
use macrotis::{MacrotisConfig};
use macrotis::resource::{Resource, ResHash};
use macrotis::tinydns;
use std::collections::HashMap;
//use macrotis::MacrotisRecord;
//use std::env;
use std::fs::{File, metadata};
use std::path::Path;
use std::io::{BufReader};
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

    // Check subcommand and bail if none provided
    let sub = match matches.subcommand_name() {
        Some("lint") => 0,
        Some("noop") => 1,
        Some("execute") => 2,
        _ => {
            println!("Missing subcommand. Use 'macrotis --help' for usage");
            std::process::exit(1);
        }
    };
    
    // Load up local records based on the 'input' argument provided.
    // Bail out on error
    let local_recs = match load_local(&input, &config) {
        Some(x) => x,
        None => {
            println!("Error processing input file(s)");
            std::process::exit(1);
        }
    };
    println!("Processed {} local records.", local_recs.0.len());
    
    // Exit now if 'lint' subcommand provided
    if sub == 0 {
		return;
	}

    // Load and parse statefile to populate 'state' - Note that state could
    // be empty if this is the first run!
    let st = match state::load_state(&config) {
        Some(x) => x,
        None => {
            println!("Error processing statefile, bailing out.");
            std::process::exit(1);
        }
    };
    println!("Statefile: {}", st);
    let mut state_recs = st.records;
    

    // Load and parse remote provider zones to populate 'remote' - Note that
    // these could also be empty!  Bail out on errors.
    let remote_recs = match load_remote(&config) {
        Some(x) => x,
        None => {
            println!("Error downloading remote records, bailing out.");
            std::process::exit(1);
        }
    };
    println!("Got {} resources from remote", remote_recs.0.len());

    // Compare statefile records with remote records to ensure state accurately
    // reflects the 'source of truth'
    compare::state_remote(&mut state_recs, &remote_recs);

    // Compare local records with updated statefile records to see what changes
    // need to be sent to remote.
    let (mut new_recs, mut upd_recs, del_recs) = compare::local_state(&local_recs, &state_recs);

    // Compare the 'new' change set to the remote records, since it contains
    // records the statefile is unaware of but which might already exist
    // remotely.
    compare::new_remote(&mut new_recs, &mut upd_recs, &remote_recs);
    
    // Print out changes to be pushed
    output_changes(&new_recs, &upd_recs, &del_recs, &state_recs);

    // Exit now if 'noop' subcommand provided
    if sub != 2 {
		return;
	}
	
	// Turn those ResHashes into something a little more palatable - 
	// simple &str,Vec<Resource> hashes where the &str part matches
	// an AWS action (CREATE, UPSERT, DELETE).
	let mut to_push: HashMap<&str, Vec<Resource>> = HashMap::new();
	to_push.insert("CREATE", resource::hash_to_vec(new_recs));
	to_push.insert("UPSERT", resource::hash_to_vec(upd_recs));
	to_push.insert("DELETE", resource::hash_to_vec(del_recs));
	
    // Finally, send the changes up to the remote provider
    match push_remote(&config, &to_push) {
		true => {
			println!("Successfully pushed changes.");
		},
		false => {
			println!("Error pushing changes, bailing out.");
			std::process::exit(1);
		}
	};
	
    // Make the current local into the new state and write the new statefile
    state::save_state(&config, local_recs);
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
fn load_local(fname: &str, config: &MacrotisConfig) -> Option<ResHash> {
    // Check if input is a dir or a file using std::fs::metadata
    // call .is_dir() or .is_file() for an appropriate bool
    let meta = match metadata(&fname) {
        Ok(x) => x,
        Err(e) => {
            println!("Error reading {}: {}", fname, e);
            std::process::exit(1);
        }
    };

    // Call tinydns::from_file either once (is_file) or in a loop
    // (is_dir).
    if meta.is_file() {
        println!("Processing {}", &fname);
        let tdns_records = match tinydns::from_file(&fname) {
            Some(x) => x,
            None => {
                println!("Error processing input file {}", fname);
                return None;
            }
        };
        println!("Converting TinyDNS records...");
        let converted = match resource::vec_from_tiny(&tdns_records, &config.zones) {
            Some(x) => x,
            None => {
                println!("Error converting TDRs to Resources");
                return None;
            }
        };
        let retval = match resource::build_reshash(converted) {
			Some(x) => x,
			None => {
				println!("Error building ResHash");
				return None;
			}
		};
		return Some(retval);
    } else {
        // Get a list of *.tiny files in the directory and call the tinydns
        // functions as necessary.
        // This is kinda gross???
        let mut error_flag = false;
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
                            let mut recs = match tinydns::from_file(&pathstring) {
                                Some(x) => x,
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
            let converted = match resource::vec_from_tiny(&tdns_vec, &config.zones) {
                Some(x) => x,
                None => {
                    println!("Error converting TDRs to Resources");
                    return None;
                }
            };
            let retval = match resource::build_reshash(converted) {
				Some(x) => x,
				None => {
					println!("Error building ResHash");
					return None;
				}
			};
			match error_flag {
				true => { return None; },
				false => { return Some(retval); }
			};
        } else {
            println!("Error getting iterator for {}", path.display());
            return None;
        }

    }
}

// Load and parse remote records
fn load_remote(config: &MacrotisConfig) -> Option<ResHash> {
    let prov = &config.provider;
    let mut resources = Vec::new();
    for z in &config.zones {
		match r53::bulk_fetch(prov, &z.id) {
			Some(mut x) => { resources.append(&mut x); },
			None => { println!("No records for zone {}", z.name); }
		};
	}
    let retval = match resource::build_reshash(resources) {
		Some(x) => x,
		None => {
			println!("Error building ResHash");
			return None;
		}
	};
    Some(retval)
}


// Push records up to remote
// 'resources' should be a HashMap where the key is an action to take
// (create, upsert, delete), and the values are Vecs of Resources
fn push_remote(config: &MacrotisConfig, resources: &HashMap<&str,Vec<Resource>>) -> bool {
	let mut retval = true;
	let prov = &config.provider;
	let mut by_zone: HashMap<&str, Vec<rusoto_route53::Change>> = HashMap::new();
	
	// So for each of the possible actions, we want to turn the Resource
	// struct into a rusoto_r53::Change struct, while simultaneously
	// separating the Resources by their zone_id.  Because Route53 
	// allows us to send multiple types of changes together so long as
	// they are all within a single HostedZone, we should be able to do
	// something that goes...a little bit a-like a-dis:
	for (action, res) in resources {
		for rec in res {
			let z = &rec.zone_id[..];
			let chg = r53::resource_to_change(&action, &rec);
            by_zone.entry(z.clone()).or_insert(vec![]).push(chg);
		}
	}
	
	// Now iterate through that by_zone hashmap and call bulk_put for
	// each one.
	for (zoneid, chgvec) in by_zone {
		match r53::bulk_put(&prov, chgvec, &zoneid) {
			Ok(x) => { println!("Change IDs: {}", x); },
			Err(e) => { println!("Error! {}", e); retval = false; }
		};
	}
    retval
}

// Iterate through the ResHashes of changes and print out what needs to
// be done to bring Remote in line with Local.  Returns 'false' if there
// are no changes to push.
fn output_changes(ne: &ResHash, up: &ResHash, de: &ResHash, st: &ResHash) -> bool {
	for (_k, v) in &ne.0 {
		println!("[ADD] {} {}\t [ ] -> {:?}", &v.rtype, &v.name, &v.records);
	}
	for (k, v) in &up.0 {
		let oldres = match st.0.get(k) {
			Some(x) => x,
			None => {
				println!("Failed to get value for key {} in state", k);
				continue;
			}
		};
		println!("[UPD] {} {}\t {:?} -> {:?}", &v.rtype, &v.name, &oldres.records, &v.records);
	}
	for (_k, v) in &de.0 {
		println!("[DEL] {} {}\t {:?} -> [ ]", &v.rtype, &v.name, &v.records);
	}
	if ne.0.len() < 1 && up.0.len() < 1 && de.0.len() < 1 {
		println!("No changes detected.");
		false
	} else {
		true
	}
}
