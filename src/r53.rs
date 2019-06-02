// Functions for talking to Route53
use std::str::FromStr;
use {MacrotisRecord, MacrotisProviderConfig};
use rusoto_core::{Region, HttpClient};
use rusoto_route53::{Route53Client, Route53, ListResourceRecordSetsRequest};
use rusoto_route53::ResourceRecordSet;
use rusoto_sts::{StsClient, StsAssumeRoleSessionCredentialsProvider};

// Retrieve all records for a given zone ID
// Returns a Vec of MacrotisRecord structs
pub fn bulk_fetch(conf: &MacrotisProviderConfig, zone_id: &str) -> Option<Vec<MacrotisRecord>> {
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
            Region::default());
    }

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
                
// Take a Vec of Route53 ResourceRecordSet structs, convert to a Vec of
// MacrotisRecord structs
fn parse_records(records: Vec<ResourceRecordSet>, zone: &str) -> Vec<MacrotisRecord> {
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
        let mac_rec = MacrotisRecord {
            zone_id: zone.to_string(),
            name: name.to_string(),
            rtype: rtype.to_string(),
            records: values,
            ttl: ttl
        };
        retval.push(mac_rec);
    }
    retval
}

