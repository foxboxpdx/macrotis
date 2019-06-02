use super::R53Zone;
use std::cmp::Ordering;
pub mod parser;
pub mod converter;

#[derive(Debug, Hash)]
pub struct TinyDNSRecord {
    pub rtype: String,
    pub fqdn: String,
    pub target: String,
    pub ttl: i32,
}

impl Eq for TinyDNSRecord {}

impl PartialEq for TinyDNSRecord {
    // For all intents and purposes, a record can be considered a duplicate
    // regardless of the TTL.
    fn eq(&self, other: &Self) -> bool {
        self.rtype  == other.rtype &&
        self.fqdn   == other.fqdn &&
        self.target == other.target
    }
}

impl Ord for TinyDNSRecord {
    fn cmp(&self, other: &TinyDNSRecord) -> Ordering {
        self.fqdn.cmp(&other.fqdn)
    }
}

impl PartialOrd for TinyDNSRecord {
    fn partial_cmp(&self, other: &TinyDNSRecord) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for TinyDNSRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}\t{}\tIN\t{}\t{}", self.fqdn, self.ttl, self.rtype, self.target)
    }
}

impl TinyDNSRecord {
    // Given a Vec of R53Zones (ie, from the config file), try to figure out
    // which zone_id should be assigned by comparing the TinyDNSRecord's fqdn
    // to the domains of the Zones and choosing the longest match.  Returns
    // Some(String) containing a zone id, or None.
    fn find_zone_id(&self, zones: &Vec<R53Zone>) -> Option<String> {
        let mut match_tuple = ("X".to_string(), None);
        for z in zones {
            if self.fqdn.contains(&z.domain) {
                // If zone domain was found inside self.fqdn, compare the length
                // of the domain name against any prior matches.  If it's 
                // longer, this is a more likely match, and should replace the
                // prior match.
                if z.domain.len() > match_tuple.0.len() {
                    match_tuple = (z.domain.to_string(), Some(z.id.to_string()));
                }
            }
        }
        // Just return whatever's in the second spot of match_tuple
        match_tuple.1
    }
}
