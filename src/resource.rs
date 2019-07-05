// Module defining operations with Resource structs

use std::collections::HashMap;
use tinydns::TinyDNSRecord;
use tinydns;
use Zone;

// What is a resource?  Dns data with a zone_id attached.
#[derive(Serialize, Deserialize, Debug, Hash, Clone)]
pub struct Resource {
    pub zone_id: String,
    pub name: String,
    pub rtype: String,
    pub records: Vec<String>,
    pub ttl: i64
}

// A collection of Resources uses the type+name to generate a unique
// 'key' for easy comparison and iteration
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResHash(pub HashMap<String, Resource>);

// Implement Equality operations
impl Eq for Resource { }

impl PartialEq for Resource {
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
impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}\t{}\tIN\t{}\t{:?}", self.name, self.ttl, self.rtype, self.records)
    }
}

impl Resource {
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

// Build a ResHash from a Vec of Resources.  Combine the records Vecs
// of any duplicate names, unless they are PTRs, then complain.
pub fn build_reshash(records: Vec<Resource>) -> Option<ResHash> {
	let mut hash: HashMap<String, Resource> = HashMap::new();
	let mut error_flag = false;
	
	for mut rec in records {
		// Generate a string from the resource type and name to serve as
		// a unique identifier/hashmap key.  Clean up any trailing dots.
		let mut record_name = format!("{}-{}", &rec.rtype, &rec.name);
		record_name = record_name.trim_end_matches('.').to_string();
		record_name = record_name.replace(".", "-").to_ascii_lowercase();
		
		// Check for an existing resource in the hashmap.  Merge the
		// 'records' arrays (unless it's a PTR, then complain).
		if hash.contains_key(&record_name) {
			let old_record = hash.remove(&record_name).unwrap();
			if rec.rtype.as_str() == "PTR" {
				println!("Error: Duplicate PTR Record:");
				println!("< {}\n> {}", old_record, rec);
				println!("HINT: Replace '=' with '+' in tinydns file");
				error_flag = true;
			} else {
				if !rec.merge(&old_record) {
					println!("Error merging records:");
					println!("< {}\n> {}", old_record, rec);
					error_flag = true;
				}
			}
		}
		
		// Add this name/resource combo to the hash
		hash.insert(record_name, rec);
	}
	
	
	if error_flag {
		None
	} else {
		Some(ResHash{ 0: hash })
  }
}

// Build a Vec of Resources from a Vec of TinyDNSRecords when supplied
// with Zone metadata
pub fn vec_from_tiny(records: &Vec<TinyDNSRecord>, zones: &Vec<Zone>) -> Option<Vec<Resource>> {
	let mut retval = Vec::new();
	// A flag in case any problems are encountered
	let mut error_flag = false;
	
	// For each TDR, find its zone_id and build a Resource struct
	for rec in records {
		let zone_id = match tinydns::find_zone_id(&rec, &zones) {
			Some(x) => x,
			None => {
				println!("Warning: Unable to find zone_id for {}", rec.fqdn);
				error_flag = true;
				continue;
			}
		};
		
		// Create a Resource struct
		let res = Resource {
			zone_id: zone_id.to_string(),
			name:    rec.fqdn.to_string(),
			rtype:   rec.rtype.to_string(),
			records:  vec![rec.target.to_string()],
			ttl:     rec.ttl as i64
		};
		retval.push(res);
	}
	if error_flag {
		None
	} else {
		Some(retval)
	}
}

// Turn a ResHash into just a Vec of Resources. Because I need to do
// that for some reason.  Consumes the ResHash, returns a
// Vec<Resource>.
pub fn hash_to_vec(hsh: ResHash) -> Vec<Resource> {
	let mut retval: Vec<Resource> = Vec::new();
	for (_k, v) in hsh.0 {
		retval.push(v);
	}
	retval
}
