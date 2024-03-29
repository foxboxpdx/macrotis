// Functions for talking to Route53
use std::str::FromStr;
use MacrotisProviderConfig;
use resource::Resource;
use rusoto_core::{Region, HttpClient};
use rusoto_route53::{Route53Client, Route53, ListResourceRecordSetsRequest};
use rusoto_route53::{ResourceRecord, ResourceRecordSet, Change};
use rusoto_route53::{ChangeBatch, ChangeResourceRecordSetsRequest};
use rusoto_sts::{StsClient, StsAssumeRoleSessionCredentialsProvider};


// Build a Route53Client for Route53 operations
pub fn build_client(conf: &MacrotisProviderConfig) -> Option<Route53Client> {
	// Grab region from conf or use the default
	let region = match &conf.region {
		Some(x) => Region::from_str(&x).unwrap_or(Region::default()),
		None => Region::default()
	};
	
	let mut client = Route53Client::new(region.to_owned());
	
	// Test to see if we need to assume a role using STS
	if conf.assume_role == true {
		let arn = match &conf.role_arn {
			Some(x) => x.to_string(),
			None => {
				println!("Assume_Role = true but no role_arn given?");
				return None;
			}
		};
		let session = match &conf.session_name {
			Some(x) => x.to_string(),
			None => "default".to_string()
		};
		let sts = StsClient::new(region.to_owned());
		let provider = StsAssumeRoleSessionCredentialsProvider::new(
		    sts,
		    arn,
		    session,
		    None, None, None, None
		);
		client = Route53Client::new_with(
			HttpClient::new().unwrap(),
			provider,
			region);
	}
	Some(client)
}

// Retrieve all records for a given zone ID
// Returns a Vec of MacrotisRecord structs
pub fn bulk_fetch(conf: &MacrotisProviderConfig, zone_id: &str) -> Option<Vec<Resource>> {
    // Build the client
    let client = match build_client(&conf) {
        Some(x) => x,
        None => {
            println!("Error creating Route53 Client");
            return None;
        }
    };

    let mut retval = Vec::new();

    // Begin retrieving records 100 at a time
    // Loop until is_truncated comes back false
    let mut req = ListResourceRecordSetsRequest {
        hosted_zone_id: zone_id.to_string(),
        max_items: None, start_record_identifier: None,
        start_record_type: None, start_record_name: None };
    loop {
        match client.list_resource_record_sets(req.to_owned()).sync() {
            Err(e) => {
                println!("Error fetching from Route53: {}", e);
                return None;
            },
            Ok(output) => {
                let mut current_batch = parse_records(output.resource_record_sets, &zone_id);
                retval.append(&mut current_batch);
                if output.is_truncated {
                    req.start_record_name = output.next_record_name;
                    req.start_record_type = output.next_record_type;
                    req.start_record_identifier = output.next_record_identifier;
                } else {
                    break;
                }
            }
        }
    }

    // Return the parsed record vector
    Some(retval)
}
  
// Given Provider metadata, a zone_id, and a vector of changes to push,
// generate a number of Route53 requests and push everything up there.
pub fn bulk_put(conf: &MacrotisProviderConfig, mut records: Vec<Change>, zone_id: &str) -> Result<String, String> {
    // Build the client
    let client = match build_client(&conf) {
        Some(x) => x,
        None => {
            return Err("Error creating Route53 Client".to_string());
        }
    };
        
    // We can only send 100 items at a time, so use vec.split_off to 
    // shift them into their own temp vec.  split_off panics if given
    // a number larger than vec.len so do some checking there first.
    loop {
		let c = records.len();
        let chunk = records.split_off(std::cmp::min(c, 99));
		let batch = ChangeBatch { changes: chunk, comment: None };
		let req = ChangeResourceRecordSetsRequest {
			change_batch: batch,
			hosted_zone_id: zone_id.to_string()
		};
		match client.change_resource_record_sets(req.to_owned()).sync() {
			Err(e) => {
				println!("Error sending changes to Route53: {}", e);
				return Err(e.to_string());
			},
			Ok(output) => {
				let id = output.change_info.id;
				println!("{}", id);
			}
		};
		if records.is_empty() {
			break;
		}
	}
	Ok("Success".to_string())		
}
              
// Take a Vec of Route53 ResourceRecordSet structs, convert to a Vec of
// MacrotisRecord structs
fn parse_records(records: Vec<ResourceRecordSet>, zone: &str) -> Vec<Resource> {
    let mut retval = Vec::new();

    // Iterate and process
    for rec in records {
        let name = rec.name;
        let rtype = rec.type_;
        let ttl = rec.ttl.unwrap_or(300); // Default to 300s if ttl is None
        // resource_records is an Option<Vec<ResourceRecord>>
        let mut values = Vec::new();
        match rec.resource_records {
            Some(x) => {
                for r in x {
                    values.push(r.value);
                }
            },
            None => {}
        };
        let mac_rec = Resource {
            zone_id: zone.to_string(),
            name: name.trim_end_matches('.').to_string(),
            rtype: rtype.to_string(),
            records: values,
            ttl: ttl
        };
        retval.push(mac_rec);
    }
    retval
}

// Given a Vec of Macrotis Resources and an action, generate a Vec of
// rusoto_r53 Change structs (consisting of a String and a 
// rusoto_r53 ResourceRecordSet)
pub fn macrotis_to_r53(resources: &Vec<Resource>, action: &str) -> Vec<Change> {
	let mut retval = Vec::new();
	for res in resources {
		// Turn the records part into an array of hashes for some
		// godforsaken reason
		let mut rrvec: Vec<ResourceRecord> = Vec::new();
		for rec in &res.records {
			let rr = ResourceRecord { value: rec.to_string() };
			rrvec.push(rr);
		}
		let rrs = ResourceRecordSet {
			name: res.name.to_string(),
			type_: res.rtype.to_string(),
			ttl: Some(res.ttl),
			resource_records: Some(rrvec),
			..Default::default()
		};
		let change = Change {
			action: action.to_string(),
			resource_record_set: rrs
		};
		retval.push(change);
	}
	retval
}

pub fn resource_to_change(action: &str, res: &Resource) -> Change {
	let mut rrvec: Vec<ResourceRecord> = Vec::new();
	for rec in &res.records {
		let rr = ResourceRecord { value: rec.to_string() };
		rrvec.push(rr);
	}
	let rrs = ResourceRecordSet {
		name: res.name.to_string(),
		type_: res.rtype.to_string(),
		ttl: Some(res.ttl),
		resource_records: Some(rrvec),
		..Default::default()
	};
	Change {
		action: action.to_string(),
		resource_record_set: rrs
	}
}
