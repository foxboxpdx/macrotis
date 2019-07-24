// Module defining comparison operations using ResHashes
use resource::{ResHash, Resource};
use std::collections::HashMap;

// Compare records from a statefile with records retrieved from the
// remote server.  Anything in state that differs from remote should be
// corrected and the user informed about it.
pub fn state_remote(st: &mut ResHash, re: &ResHash) {
	let (mut del, mut upd) = (Vec::new(), Vec::new());
	for (key, rec) in st.0.clone() {
		if re.0.contains_key(&key) {
			let remote = re.0.get(&key).unwrap();
			if &rec != remote {
				println!("[WARNING] Remote record {} does not match statefile", &key);
				println!("Statefile: {}\nRemote: {}", rec, remote);
				upd.push(key.clone());
			}
		} else {
			println!("[WARNING] Record {} appears in state but not remote", &key);
			del.push(key.clone());
		}
	}
	for k in del {
		st.0.remove(&k).unwrap();
	}
	for k in upd {
		let x = re.0.get(&k).unwrap().clone();
		st.0.insert(k.to_string(), x);
	}
}

// Compare records from a statefile with records processed from local
// files.  Separate into three new ResHashes, containing NEW, UPDATED,
// and DELETED records.
pub fn local_state(lo: &ResHash, st: &ResHash) -> (ResHash, ResHash, ResHash) {
	let (mut n, mut u, mut d) = (HashMap::new(), HashMap::new(), HashMap::new());
	let st_hash = &st.0;
	let lo_hash = &lo.0;
	for (key, rec) in lo_hash {
        match st_hash.get(key) {
            Some(x) => { if x != rec { u.insert(key.clone(), x.clone()); },
            None => n.insert(key.clone(), x.clone())
        }
	}
	for (key, rec) in st_hash {
		if !lo_hash.contains_key(key) {
			// We need to clone 'rec' so 'd' can take ownership of it
			d.insert(key.clone(), rec.clone());
		}
	}
	(ResHash (n), ResHash (u), ResHash (d))
}

// Compare records determined to be 'NEW' with the records retrieved
// from the remote server.  Matches mean that records exist remotely
// but the statefile doesn't know about them.  Warn the user and either
// (1) Drop the record from the NEW ResHash if both are identical, or
// (2) Move the record to the UPDATE ResHash
pub fn new_remote(ne: &mut ResHash, up: &mut ResHash, re: &ResHash) {
	let mut drop = Vec::new();
	let mut mv = Vec::new();
	for (key, rec) in ne.0.clone() {
		if re.0.contains_key(&key) {
			println!("[WARNING] Record missing from statefile...");
			let remote = re.0.get(&key).unwrap();
			if &rec == remote {
				println!("but records are identical: {}", &key);
				drop.push(key.clone());
			} else {
				println!("and records differ!\nLocal: {}\nRemote: {}", &rec, &remote);
				mv.push(key.clone());
			}
		}
	}
	for k in drop {
		ne.0.remove(&k).unwrap();
	}
	for k in mv {
		let rec = ne.0.remove(&k).unwrap();
		up.0.insert(k.to_string(), rec);
	}		
}

