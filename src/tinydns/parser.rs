// Define functions for processing TinyDNS flat files
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufRead};
use std::time::SystemTime;
use std::net::Ipv4Addr;
use tinydns::TinyDNSRecord;

// Given a filename, read in the contents and generate a Vec of TDRs
pub fn from_file(fname: &str) -> Option<Vec<TinyDNSRecord>> {
    let mut retval = Vec::new();

    // Attempt to open and read file
    let f = match File::open(fname) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening file {}: {}", fname, e);
            return None;
        }
    };
    let reader = BufReader::new(&f);

    // Process each line in the file and call the appropriate parsing
    // function.  Remember that some prefixes generate more than one!
    // Because of that, all the parse_X functions return a vector that
    // can be simply append()-ed to retval. If there's an error, we simply
    // get back an empty vector.
    for line in reader.lines() {
        let l = line.expect("Couldn't get line?");
        match from_string(&l) {
            Some(mut x) => { retval.append(&mut x); }
            None => { return None; }
        }
    }

    // Check for duplicates in the Vec and warn about them
    check_dups(&retval);

    // Sort and dedupe the Vec
    retval.sort();
    retval.dedup();

    // Return the parsed records
    Some(retval)
}

// It occurs to me there might be reason to just process a single string
// instead of a whole file, so move all that matching nonsense down here and
// just call this function for each line
pub fn from_string(line: &str) -> Option<Vec<TinyDNSRecord>> {
    // Since half of these need to return more than 1 struct, they're all set
    // to return a Vec of TDRs. If that Vec is empty, there was an issue, and
    // we should return None so the upstream calling function can deal with it.
    // Comments and excluded records should still return 'successful' but empty.
    let (prefix, data) = line.split_at(1);
    let parsed = match prefix {
        "+" => { parse("A", data) },
        "^" => { parse("PTR", data) },
        "C" => { parse("CNAME", data) },
        "'" => { parse_txt(data) },
        "@" => { parse_mx(data) },
        "Z" => { parse_soa(data) },
        "." => { parse_anssoa(data) },
        "&" => { parse_ans(data) },
        "=" => { parse_aptr(data) },
        "-" => { return Some(Vec::new()); }, // Excluded record, ignore
        "#" => { return Some(Vec::new()); }, // Comment line, ignore
        _   => {
            println!("Unsuported prefix: {}", prefix);
            Vec::new()
        }
    };

    // Return parsed if there's anything in it.
    match parsed.is_empty() {
        true => None,
        false => Some(parsed)
    }
}

// For some reason trying to include this above just before the sort and 
// dedup was triggering the wrath of the borrow checker.  Could have this
// return a bool in case we want to error on duplicates.
fn check_dups(records: &Vec<TinyDNSRecord>) {
    let mut uniq = HashMap::new();
    for rec in records {
        uniq.entry(rec).or_insert(vec![]).push(rec);
    }

    for (k, v) in &uniq {
        if v.len() > 1 {
            println!("Warning, duplicate record found:\n\t{}", k);
        }
    }
}

// Parse a basic DNS record into 1 TinyDNSRecord
// +fqdn:rec:ttl:timestamp:lo - A
// ^fqdn:rec:ttl:timestamp:lo - PTR
// Cfqdn:rec:ttl:timestamp:lo - CNAME
pub fn parse(rtype: &str, data: &str) -> Vec<TinyDNSRecord> {
    // Create our return Vec
    let mut retval = Vec::new();

    // Split up the data by colon.
    let mut parts: Vec<&str> = data.split(':').collect();

    // The FQDN and Target are mandatory. Print an error and return an
    // empty Vec if there aren't at least 2 items in 'parts'
    if parts.len() < 2 {
        println!("Error parsing line: {} of type {}", data, rtype);
        return retval;
    }

    // Pull those parts out
    let fqdn = parts.remove(0);
    let rec = parts.remove(0);

    let target = rec.to_string().replace("\"", "");

    // If this is an 'A' record, we should ensure 'rec' is a valid IPv4 addr
    if rtype == "A" {
        match rec.parse::<Ipv4Addr>() {
            Ok(_) => {},
            Err(e) => {
                println!("Error processing record: {}", data);
                println!("{}", e);
                return retval;
            }
        }
    }

    // See if there's a TTL in there since it would come next
    // Assign a default value of 300 if there's none provided
    // or if it can't be parsed as an i32.
    let ttl = match parts.is_empty() {
        true => 300,
        false => {
            parts.remove(0).parse::<i32>().unwrap_or(300)
        }
    };

    // Any data that may be left in 'parts' is extraneous and unneeded,
    // so proceed on to making a TDR, put it in retval, and return.
    let tdr = TinyDNSRecord {
        rtype: rtype.to_string(),
        fqdn:  fqdn.to_string(),
        target: target,
        ttl: ttl
    };
    retval.push(tdr);

    retval
}

// Parse a TXT record - gets its own function because strings can be dumb
// 'fqdn:rec:ttl:timestamp:lo
// Type=TXT, fqdn=fqdn, target=string with extraneous quotes removed
pub fn parse_txt(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split on colon like usual, but there's a catch...
    let mut parts: Vec<&str> = data.split(':').collect();

    // There still need to be at least two things in there
    if parts.len() < 2 {
        println!("Error parsing line: {} of type TXT", data);
        return retval;
    }

    // And the first part is just fqdn as normal
    let fqdn = parts.remove(0);

    // But now we need to look for our start and end double-quotes.  If the
    // first chunk we pull out of parts starts_with and ends_with ", we're good
    // and can move on.  Otherwise we have to keep pulling chunks out until we
    // find the end quotes.
    let mut rec = parts.remove(0).to_string();
    if !rec.starts_with('"') {
        println!("TXT record missing double-quotes: {}", data);
        return retval;
    }
    while !rec.ends_with('"') {
        // Make sure there's another piece to remove
        if parts.len() == 0 {
            println!("TXT record missing end quotes: {}", data);
            return retval;
        }
        // Extract and add on to rec, then finish loop and test again.
        let rec2 = parts.remove(0);
        rec = format!("{}:{}", rec, rec2);
    }

    // That should get us the text string with colons intact.  Now remove those
    // double-quotes because otherwise serializing to JSON will make data that
    // Terraform doesn't like. Reminder this returns a &str.
    let target = rec.trim_matches('"');

    // Check for TTL
    let ttl = match parts.is_empty() {
        true => 300,
        false => {
            parts.remove(0).parse::<i32>().unwrap_or(300)
        }
    };

    // Any data that may be left in 'parts' is extraneous and unneeded,
    // so proceed on to making a TDR, put it in retval, and return.
    let tdr = TinyDNSRecord {
        rtype: "TXT".to_string(),
        fqdn:  fqdn.to_string(),
        target: target.to_string(),
        ttl: ttl
    };
    retval.push(tdr);

    // Return retval
    retval
}

// Parse an MX record into two TinyDNSRecords
// @fqdn:ip:x:dist:ttl:timestamp:lo
// (1) type=MX, fqdn=fqdn, target="dist x(.mx.fqdn)"
// (2) type=A,  fqdn=x(.mx.fqdn), target=ip
pub fn parse_mx(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split up data by colon
    let mut parts: Vec<&str> = data.split(':').collect();

    // FQDN, target, mx_fqdn required; error and return on parts < 3
    if parts.len() < 3 {
        println!("Error parsing line: {} of type MX", data);
        return retval;
    }

    // Pull out required parts
    let fqdn = parts.remove(0);
    let ip = parts.remove(0);
    let x = parts.remove(0);

    // Make sure IP is an IP
    match ip.parse::<Ipv4Addr>() {
        Ok(_) => {},
        Err(e) => {
            println!("Error processing record: {}", data);
            println!("{}", e);
            return retval;
        }
    }

    // TinyDNS spec states that if x contains a period, it is used
    // as-is; otherwise, it becomes x.mx.fqdn.
    let mx_fqdn = match x.to_string().contains('.') {
        true => x.to_string(),
        false => format!("{}.mx.{}", x, fqdn)
    };

    // Do some fancy matching footwork to populate the mx_dist and ttl
    // depending on whether they were provided. Even though mx_dist will
    // wind up as part of a string, make sure it's a valid integer first.
    let (mx_dist, ttl) = match parts.len() {
        0 => (0, 300),
        1 => (parts.remove(0).parse::<i32>().unwrap_or(0), 300),
        _ => (parts.remove(0).parse::<i32>().unwrap_or(0),
              parts.remove(0).parse::<i32>().unwrap_or(300))
    };

    // Generate MX TDR
    let tdr1 = TinyDNSRecord {
        rtype:   "MX".to_string(),
        fqdn:    fqdn.to_string(),
        target:  format!("{} {}", mx_dist, mx_fqdn),
        ttl:     ttl
    };
    retval.push(tdr1);

    // Generate A TDR
    let tdr2 = TinyDNSRecord {
        rtype:  "A".to_string(),
        fqdn:   mx_fqdn,
        target: ip.to_string(),
        ttl:    ttl
    };
    retval.push(tdr2);

    // Return Vec
    retval
}

// Parse an SOA record 
// Zfqdn:ns:contact:serial:refresh:retry:expire:min:ttl:timestamp:lo
// serial, refresh, retry, expire, and min are optional and default to
// epoch, 16384, 2048, 1048576, and 2560.
pub fn parse_soa(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split on colon
    let mut parts: Vec<&str> = data.split(':').collect();

    // Error and return if we don't have at least 3 items
    if parts.len() < 3 {
        println!("Error parsing line: {} of type SOA", data);
        return retval;
    }

    // Pull the required 3 off
    let fqdn    = parts.remove(0);
    let ns      = parts.remove(0);
    let contact = parts.remove(0);

    // As with MX, we can do some fancy footwork with match based on how
    // many items are left in the parts vector.  Start by getting an
    // epoch time in case we need it.
    let right_now = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => panic!("Something is REALLY wrong, SystemTime < EPOCH??")
    };

    // Now the match game. Again these wind up in a string but we want to
    // ensure they are valid integers first.
    let (ser, refr, retr, exp, min, ttl) = match parts.len() {
        0 => (right_now, 16384, 2048, 1048576, 2560, 300),
        1 => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              16384, 2048, 1048576, 2560, 300),
        2 => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              parts.remove(0).parse::<i32>().unwrap_or(16384),
              2048, 1048576, 2560, 300),
        3 => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              parts.remove(0).parse::<i32>().unwrap_or(16384),
              parts.remove(0).parse::<i32>().unwrap_or(2048),
              1048576, 2560, 300),
        4 => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              parts.remove(0).parse::<i32>().unwrap_or(16384),
              parts.remove(0).parse::<i32>().unwrap_or(2048),
              parts.remove(0).parse::<i32>().unwrap_or(1048576),
              2560, 300),
        5 => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              parts.remove(0).parse::<i32>().unwrap_or(16384),
              parts.remove(0).parse::<i32>().unwrap_or(2048),
              parts.remove(0).parse::<i32>().unwrap_or(1048576),
              parts.remove(0).parse::<i32>().unwrap_or(2560), 300),
        _ => (parts.remove(0).parse::<u64>().unwrap_or(right_now),
              parts.remove(0).parse::<i32>().unwrap_or(16384),
              parts.remove(0).parse::<i32>().unwrap_or(2048),
              parts.remove(0).parse::<i32>().unwrap_or(1048576),
              parts.remove(0).parse::<i32>().unwrap_or(2560),
              parts.remove(0).parse::<i32>().unwrap_or(300))
    };
    // That could probably be a lot cleaner.  Oh well.

    // Generate that target string
    let target = format!("{} {} {} {} {} {} {}", ns, contact, ser, refr, 
                         retr, exp, min);

    // Generate TDR, push, return
    let tdr = TinyDNSRecord {
        rtype:  "SOA".to_string(),
        fqdn:   fqdn.to_string(),
        target: target,
        ttl:    ttl
    };
    retval.push(tdr);

    // Return
    retval
}

// Parse a combination A/NS/SOA record into 3 TinyDNSRecords
// .fqdn:ip:x:ttl:timestamp:lo
// (1) type=NS, fqdn=x(.ns.fqdn), target=fqdn
// (2) type=A,  fqdn=x(.ns.fqdn), target=ip
// (3) type=SOA fqdn=fqdn, target="x hostmaster.fqdn default-values"
pub fn parse_anssoa(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split on colon
    let mut parts: Vec<&str> = data.split(':').collect();

    // Make sure there's enough pieces
    if parts.len() < 3 {
        println!("Error parsing line: {} of type A/NS/SOA", data);
        return retval;
    }

    // Get 'em
    let fqdn = parts.remove(0);
    let ip = parts.remove(0); // This can be empty
    let x = parts.remove(0);

    // Make sure IP is an IP
    match ip.parse::<Ipv4Addr>() {
        Ok(_) => {},
        Err(e) => {
            println!("Error processing record: {}", data);
            println!("{}", e);
            return retval;
        }
    }

    // Thankfully there's no big ugly match chains here, just a boolean
    let ttl = match parts.is_empty() {
        true => 300,
        false => parts.remove(0).parse::<i32>().unwrap_or(300)
    };

    // As with MX, if x contains a period, it is used as is; otherwise, it
    // becomes x.ns.fqdn.
    let ns_fqdn = match x.to_string().contains('.') {
        true => x.to_string(),
        false => format!("{}.ns.{}", x, fqdn)
    };

    // Start building TDRs. If ip is empty, don't create (2).
    let tdr1 = TinyDNSRecord {
        rtype:  "NS".to_string(),
        fqdn:   ns_fqdn.to_string(),
        target: fqdn.to_string(),
        ttl:    ttl
    };
    retval.push(tdr1);

    if !ip.is_empty() {
        let tdr2 = TinyDNSRecord {
            rtype:  "A".to_string(),
            fqdn:   ns_fqdn.to_string(),
            target: ip.to_string(),
            ttl:    ttl
        };
        retval.push(tdr2);
    }

    let target = format!("{} hostmaster.{} 1 1 1 1 60", &ns_fqdn, &fqdn);
    let tdr3 = TinyDNSRecord {
        rtype:  "SOA".to_string(),
        fqdn:   fqdn.to_string(),
        target: target,
        ttl:    ttl
    };
    retval.push(tdr3);

    // Return
    retval
}

// Parse a combination A/NS record into 2 TinyDNSRecords
// &fqdn:ip:x:ttl:timestamp:lo
// (1) type=NS, fqdn=x(.ns.fqdn), target=fqdn
// (2) type=A,  fqdn=x(.ns.fqdn), target=ip
pub fn parse_ans(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split on colon
    let mut parts: Vec<&str> = data.split(':').collect();

    // 3 shall be the number of the counting
    if parts.len() < 3 {
        println!("Error parsing line: {} of type A/NS", data);
        return retval;
    }

    // You're gonna extract HIM?
    let fqdn = parts.remove(0);
    let ip = parts.remove(0);
    let x = parts.remove(0);

    // Make sure IP is an IP
    match ip.parse::<Ipv4Addr>() {
        Ok(_) => {},
        Err(e) => {
            println!("Error processing record: {}", data);
            println!("{}", e);
            return retval;
        }
    }

    // Check for TTL
    let ttl = match parts.is_empty() {
        true => 300,
        false => parts.remove(0).parse::<i32>().unwrap_or(300)
    };

    // Check x for dots
    let ns_fqdn = match x.to_string().contains('.') {
        true => x.to_string(),
        false => format!("{}.ns.{}", x, fqdn)
    };

    // Build TDRs
    let tdr1 = TinyDNSRecord {
        rtype:  "NS".to_string(),
        fqdn:   ns_fqdn.to_string(),
        target: fqdn.to_string(),
        ttl:    ttl
    };
    retval.push(tdr1);

    let tdr2 = TinyDNSRecord {
        rtype:  "A".to_string(),
        fqdn:   ns_fqdn.to_string(),
        target: ip.to_string(),
        ttl:    ttl
    };
    retval.push(tdr2);

    // Return
    retval
}

// Parse a combination A/PTR record into 2 TinyDNSRecords
// =fqdn:ip:ttl:timestamp:lo
// (1) type=A, fqdn=fqdn, target=ip
// (2) type=PTR, fqdn=arpaized-ip, target=fqdn
pub fn parse_aptr(data: &str) -> Vec<TinyDNSRecord> {
    // Create return vec
    let mut retval = Vec::new();

    // Split on colon
    let mut parts: Vec<&str> = data.split(':').collect();

    // It takes two to tango
    if parts.len() < 2 {
        println!("Error parsing line: {} of type A/PTR", data);
        return retval;
    }

    // Front and back
    let fqdn = parts.remove(0);
    let ip = parts.remove(0);

    // Make sure IP is an IP
    match ip.parse::<Ipv4Addr>() {
        Ok(_) => {},
        Err(e) => {
            println!("Error processing record: {}", data);
            println!("{}", e);
            return retval;
        }
    };

    // TTL check
    let ttl = match parts.is_empty() {
        true => 300,
        false => parts.remove(0).parse::<i32>().unwrap_or(300)
    };

    // Build a PTR FQDN from the IP
    let mut ipbits: Vec<&str> = ip.split('.').collect();
    ipbits.reverse();
    let backwards = ipbits.join(".");
    let ptr_fqdn = format!("{}.in-addr.arpa", backwards);

    // Build TDRs
    let tdr1 = TinyDNSRecord {
        rtype:  "A".to_string(),
        fqdn:   fqdn.to_string(),
        target: ip.to_string(),
        ttl:    ttl
    };
    retval.push(tdr1);

    let tdr2 = TinyDNSRecord {
        rtype:  "PTR".to_string(),
        fqdn:   ptr_fqdn,
        target: fqdn.to_string(),
        ttl:    ttl
    };
    retval.push(tdr2);

    // Return
    retval
}

// How about some tests everyone loves tests!
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        // Test the 3 most basic record types
        let arec = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let prec = TinyDNSRecord {
            rtype: "PTR".to_string(),
            fqdn:  "4.3.2.1.in-addr.arpa".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        let crec = TinyDNSRecord {
            rtype: "CNAME".to_string(),
            fqdn:  "bar.test.com".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        
        let atext = "foo.test.com:1.2.3.4:300";
        let ptext = "4.3.2.1.in-addr.arpa:foo.test.com:300";
        let ctext = "bar.test.com:foo.test.com:300";

        assert!(vec![arec] == parse("A", atext));
        assert!(vec![prec] == parse("PTR", ptext));
        assert!(vec![crec] == parse("CNAME", ctext));
    }

    #[test]
    fn test_bad_ip_a_record() {
        // Make sure a bad IP in an A record returns an empty vec
        let atext="foo.test.com:999.999.999.999:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse("A", atext));
    }

    #[test]
    fn test_basic_bad_input() {
        // Make sure we get an empty vec back if we send bad data to parse()
        let text = "this is some crappy data";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse("A", text));
    }

    #[test]
    fn test_parse_txt() {
        // Test parse_text with good data
        let trec = TinyDNSRecord {
            rtype: "TXT".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "a string of data".to_string(),
            ttl: 300 };
        let text = "foo.test.com:\"a string of data\":300";

        assert!(vec![trec] == parse_txt(text));
    }

    #[test]
    fn test_bad_parse_txt() {
        // Test parse_text with bad data
        let text = "foo.test.com:no quotes uhoh:300";
        let text2 = "foo.test.com:\"missing end quote:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_txt(text));
        assert!(empty == parse_txt(text2));
    }

    #[test]
    fn test_parse_mx() {
        // Test parse_mx with good data
        let mx = TinyDNSRecord {
            rtype: "MX".to_string(),
            fqdn:  "test.com".to_string(),
            target: "20 foo.test.com".to_string(),
            ttl: 300 };
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let line = "test.com:1.2.3.4:foo.test.com:20:300";
        let parsed = parse_mx(line);
        assert!(mx == parsed[0]);
        assert!(a  == parsed[1]);
    }

    #[test]
    fn test_bad_parse_mx() {
        // Test parse_mx with bad data
        let badip = "test.com:999.999.999.999:foo.test.com:20:300";
        let badstr = "bad data";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_mx(badip));
        assert!(empty == parse_mx(badstr));
    }

    #[test]
    fn test_parse_soa() {
        // Test parse_soa with good data
        let soa = TinyDNSRecord {
            rtype: "SOA".to_string(),
            fqdn:  "test.com".to_string(),
            target: "foo.test.com person.test.com 1 2 3 4 5".to_string(),
            ttl: 300 };
        let line = "test.com:foo.test.com:person.test.com:1:2:3:4:5:300";
        assert!(vec![soa] == parse_soa(line));
    }

    #[test]
    fn test_bad_parse_soa() {
        // Test parse_soa with bad data
        let line = "look at this bad data";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_soa(line));
    }

    #[test]
    fn test_parse_anssoa() {
        // Test parse_anssoa with good data - 3 records
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ns  = TinyDNSRecord {
            rtype: "NS".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "test.com".to_string(),
            ttl: 300 };
        let soa = TinyDNSRecord {
            rtype: "SOA".to_string(),
            fqdn:  "test.com".to_string(),
            target: "foo.test.com hostmaster.test.com 1 1 1 1 60".to_string(),
            ttl: 300 };
        let line = "test.com:1.2.3.4:foo.test.com:300";
        let parsed = parse_anssoa(line);
        assert!(ns == parsed[0]);
        assert!(a  == parsed[1]);
        assert!(soa == parsed[2]);
    }

    #[test]
    fn test_bad_parse_anssoa() {
        // Test parse_anssoa with bad data
        let line = "super bad data";
        let badip = "fqdn:999.999.999.999:x:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_anssoa(line));
        assert!(empty == parse_anssoa(badip));
    }

    #[test]
    fn test_parse_ans() { 
        // Test parse_ans with good data
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ns  = TinyDNSRecord {
            rtype: "NS".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "test.com".to_string(),
            ttl: 300 };
        let line = "test.com:1.2.3.4:foo.test.com:300";
        let parsed = parse_ans(line);
        assert!(ns == parsed[0]);
        assert!(a  == parsed[1]);
    }

    #[test]
    fn test_bad_parse_ans() {
        // Test parse_ans with bad data
        let line = "no good rotten data";
        let badip = "fqdn:9999.999.258.0:x:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_ans(line));
        assert!(empty == parse_ans(badip));
    }

    #[test]
    fn test_parse_aptr() { 
        // Test parse_aptr with good data
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ptr = TinyDNSRecord {
            rtype: "PTR".to_string(),
            fqdn:  "4.3.2.1.in-addr.arpa".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        let line = "foo.test.com:1.2.3.4:300";
        let parsed = parse_aptr(line);
        assert!(a == parsed[0]);
        assert!(ptr == parsed[1]);
    }

    #[test]
    fn test_bad_parse_aptr() { 
        // Test parse_aptr with bad data
        let line = "oooooh this data!";
        let badip = "fqdn:99.999.598.10:x:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        assert!(empty == parse_aptr(line));
        assert!(empty == parse_aptr(badip));
    }

    // Bring it all together and make sure from_string() can handle the 12
    // possible arms of its match{} statement.  Most of this is just repeated
    // code from testing the individual parsing functions only passed to
    // from_string() instead of parse_X.
    #[test]
    fn test_from_string_a() {
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let line = "+foo.test.com:1.2.3.4:300";
        let parsed = from_string(line).unwrap();
        assert!(a == parsed[0]);
    }

    #[test]
    fn test_from_string_ptr() {
        let prec = TinyDNSRecord {
            rtype: "PTR".to_string(),
            fqdn:  "4.3.2.1.in-addr.arpa".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        let line = "^4.3.2.1.in-addr.arpa:foo.test.com:300";
        let parsed = from_string(line).unwrap();
        assert!(prec == parsed[0]);
    }

    #[test]
    fn test_from_string_cname() {
        let crec = TinyDNSRecord {
            rtype: "CNAME".to_string(),
            fqdn:  "bar.test.com".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        let line = "Cbar.test.com:foo.test.com:300";
        let parsed = from_string(line).unwrap();
        assert!(crec == parsed[0]);
    }

    #[test]
    fn test_from_string_txt() {
        let trec = TinyDNSRecord {
            rtype: "TXT".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "a string of data".to_string(),
            ttl: 300 };
        let line = "'foo.test.com:\"a string of data\":300";
        let parsed = from_string(line).unwrap();
        assert!(trec == parsed[0]);
    }

    #[test]
    fn test_from_string_mx() {
        let mx = TinyDNSRecord {
            rtype: "MX".to_string(),
            fqdn:  "test.com".to_string(),
            target: "20 foo.test.com".to_string(),
            ttl: 300 };
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let line = "@test.com:1.2.3.4:foo.test.com:20:300";
        let parsed = from_string(line).unwrap();
        assert!(mx == parsed[0]);
        assert!(a  == parsed[1]);
    }

    #[test]
    fn test_from_string_soa() {
        let soa = TinyDNSRecord {
            rtype: "SOA".to_string(),
            fqdn:  "test.com".to_string(),
            target: "foo.test.com person.test.com 1 2 3 4 5".to_string(),
            ttl: 300 };
        let line = "Ztest.com:foo.test.com:person.test.com:1:2:3:4:5:300";
        let parsed = from_string(line).unwrap();
        assert!(soa == parsed[0]);
    }

    #[test]
    fn test_from_string_anssoa() {
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ns  = TinyDNSRecord {
            rtype: "NS".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "test.com".to_string(),
            ttl: 300 };
        let soa = TinyDNSRecord {
            rtype: "SOA".to_string(),
            fqdn:  "test.com".to_string(),
            target: "foo.test.com hostmaster.test.com 1 1 1 1 60".to_string(),
            ttl: 300 };
        let line = ".test.com:1.2.3.4:foo.test.com:300";
        let parsed = from_string(line).unwrap();
        assert!(ns == parsed[0]);
        assert!(a  == parsed[1]);
        assert!(soa == parsed[2]);
    }

    #[test]
    fn test_from_string_ans() {
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ns  = TinyDNSRecord {
            rtype: "NS".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "test.com".to_string(),
            ttl: 300 };
        let line = "&test.com:1.2.3.4:foo.test.com:300";
        let parsed = from_string(line).unwrap();
        assert!(ns == parsed[0]);
        assert!(a  == parsed[1]);
    }

    #[test]
    fn test_from_string_aptr() {
        let a  = TinyDNSRecord {
            rtype: "A".to_string(),
            fqdn:  "foo.test.com".to_string(),
            target: "1.2.3.4".to_string(),
            ttl: 300 };
        let ptr = TinyDNSRecord {
            rtype: "PTR".to_string(),
            fqdn:  "4.3.2.1.in-addr.arpa".to_string(),
            target: "foo.test.com".to_string(),
            ttl: 300 };
        let line = "=foo.test.com:1.2.3.4:300";
        let parsed = from_string(line).unwrap();
        assert!(a == parsed[0]);
        assert!(ptr == parsed[1]);
    }

    #[test]
    fn test_from_string_comment() {
        let line = "# A comment line";
        let line2 = "-disabled:record:300";
        let empty: Vec<TinyDNSRecord> = Vec::new();
        let parsed = from_string(line).unwrap();
        let parsed2 = from_string(line2).unwrap();
        assert!(empty == parsed);
        assert!(empty == parsed2);
    }

    #[test]
    fn test_from_string_baddata() {
        let line = "2098u983rjgq24gjadjgaNONSENSE";
        let parsed = from_string(line);
        assert!(parsed == None);
    }
}
