// Define functions for converting TinyDNSRecords to MacrotisRecords
use std::collections::HashMap;
use tinydns::TinyDNSRecord;
use R53Zone;
use MacrotisRecord;

// Given a Vec of TinyDNSRecords, and a Vec of R53Zones, generate a hashmap
// of MacrotisRecords with unique names.
pub fn tiny_to_macrotis(tdrs: Vec<TinyDNSRecord>, zones: &Vec<R53Zone>) 
                        -> Option<HashMap<String, MacrotisRecord>> {

    let mut retval: HashMap<String, MacrotisRecord> = HashMap::new();

    // In case we need to bail out
    let mut error_flag = false;

    // For each TDR, look up the zone_id from the R53Zones, combine the records
    // vectors for duplicate names, complain about duplicate PTRs.
    for rec in tdrs {
        // Find zone
        let zone_id = match rec.find_zone_id(&zones) {
            Some(x) => x,
            None => {
                println!("Warning: Unable to find zone_id for {}", rec.fqdn);
                //error_flag = true;
                continue;
            }
        };

        // Turn the target into a vector
        let rec_vec = vec![rec.target.to_string()];

        // Generate a MR struct
        let mut mac_rec = MacrotisRecord {
            zone_id: zone_id.to_string(),
            name:    rec.fqdn.to_string(),
            rtype:   rec.rtype.to_string(),
            records: rec_vec,
            ttl:     rec.ttl as i64
        };

        // Generate a String to serve as a unique name/hashmap key; clean it up
        let mut record_name = format!("{}-{}", &rec.rtype, &rec.fqdn);
        record_name = record_name.trim_end_matches('.').to_string();

        // Check for an existing record in the hashmap. If it's anything but a
        // PTR, merge the 'records' arrays, otherwise complain loudly.
        if retval.contains_key(&record_name) {
            // Unwrap is ok since we just determined this exists
            let old_record = retval.remove(&record_name).unwrap();
            if mac_rec.rtype.as_str() == "PTR" {
                println!("Error: Duplicate PTR Record:");
                println!("< {}", &old_record);
                println!("> {}", &mac_rec);
                error_flag = true;
            } else {
                if !mac_rec.merge(&old_record) {
                    println!("Error merging records:");
                    println!("< {}\n> {}", old_record, mac_rec);
                    error_flag = true;
                }
            }
        }

        // Add to return hash
        retval.insert(record_name, mac_rec);
    }

    // Return hash if no errors, None if errors detected
    match error_flag {
        true => None,
        false => Some(retval)
    }
}

