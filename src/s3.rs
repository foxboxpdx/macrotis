// Functions for talking to S3
use std::str::FromStr;
use {MacrotisStateConfig, MacrotisState};
use rusoto_core::{Region, HttpClient, RusotoError};
use rusoto_sts::{StsClient, StsAssumeRoleSessionCredentialsProvider};
use rusoto_s3::{S3Client, S3, GetObjectRequest, PutObjectRequest, GetObjectError};

// Build an S3Client for S3 operations
fn build_client(conf: &MacrotisStateConfig) -> Option<S3Client> {
    // Grab region from conf or use the default
    let region = match &conf.region {
        Some(x) => Region::from_str(&x).unwrap_or(Region::default()),
        None => Region::default()
    };

    let mut client = S3Client::new(region.to_owned());

    // See if we're assuming a role
    if let Some(arn) = &conf.role_arn {
        let session = match &conf.session_name {
            Some(x) => x.to_string(),
            None => "default".to_string()
        };
        let sts = StsClient::new(region.to_owned());
        let provider = StsAssumeRoleSessionCredentialsProvider::new(
            sts,
            arn.to_string(),
            session.to_string(),
            None, None, None, None
            );
        client = S3Client::new_with(
            HttpClient::new().unwrap(),
            provider,
            Region::default());
    }
    Some(client)
}

// Attempt to retrieve state file from S3
pub fn fetch_state_file(conf: &MacrotisStateConfig) -> Option<MacrotisState> {
    // Build the client
    let client = match build_client(&conf) {
        Some(x) => x,
        None => {
            println!("Error creating S3 Client");
            return None;
        }
    };

    // Shouldn't be able to get here without these being defined but double-
    // check and return empty sadness if they're missing.
    let bucket = match &conf.bucket {
        Some(x) => x.to_owned(),
        None => { return None; }
    };
    let key = match &conf.key {
        Some(x) => x.to_owned(),
        None => { return None; }
    };

    // Attempt to grab from S3
    let get_req = GetObjectRequest {
        bucket: bucket.to_string(),
        key: key.to_string(),
        ..Default::default()
    };

    // Get_object errors with a GetObjectError(NoSuchKey(String))
    // so an error here means the key doesn't exist.  Generate a blank
    // MacrotisState and return that.  If we get some other error, well, that's
    // a problem.
    let result = match client.get_object(get_req).sync() {
        Ok(x) => x,
        Err(RusotoError::Service(GetObjectError::NoSuchKey(_))) => {
            println!("Remote statefile not found, creating a new one...");
            let state = MacrotisState::new_empty();
            return Some(state);
        },
        Err(e) => {
            println!("Error retrieving S3 object: {}", e);
            return None;
        }
    };

    let stream = result.body.unwrap();
    let body = stream.into_blocking_read();

    // We use stream.into_blocking_read as that implements Read and we can
    // hand it off to serde_json::from_reader at that point.
    let retval: MacrotisState = match serde_json::from_reader(body) {
        Ok(x) => x,
        Err(e) => {
            println!("Error reading JSON: {}", e);
            return None;
        }
    };

    // retval should now contain the state
    Some(retval)
}

// Attempt to save a state file in S3
pub fn put_state_file(conf: &MacrotisStateConfig, state: &str) -> Result<bool, String> {
    // Starts the same as fetch - build client and check config params
    let sadness = "Missing config params".to_string();
    let client = match build_client(&conf) {
        Some(x) => x,
        None => {
            println!("Error creating S3 Client");
            return Err(sadness);
        }
    };
    let bucket = match &conf.bucket {
        Some(x) => x.to_owned(),
        None => { return Err(sadness); }
    };
    let key = match &conf.key {
        Some(x) => x.to_owned(),
        None => { return Err(sadness); }
    };
    let statevec = state.to_string().into_bytes();

    // Create the request
    let req = PutObjectRequest {
        bucket: bucket.to_string(),
        key: key.to_string(),
        body: Some(statevec.into()),
        ..Default::default()
    };

    let result = match client.put_object(req).sync() {
        Ok(_) => Ok(true),
        Err(e) => Err(e.to_string())
    };
    result
}
