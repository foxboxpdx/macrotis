# Macrotis
Stateful Bulk Route53 DNS Management via TinyDNS

## Usage
`macrotis --input <FILE/DIR> --config <FILE> [SUBCOMMAND]`

### Subcommands
`execute` - Calculate changes to be made and send them to Route53
`noop` - Calculate changes and print them out, but do not send to R53
`lint` - Validate the input file(s) only and exit

## About
Macrotis aims to provide what the Terraform AWS module is missing - the ability
to rapidly and statefully manage large numbers of DNS records in Route53 using
a simple and compact flat file format.  Future updates aim to target additional
cloud DNS providers including Azure.

### Stateful?
Yes!  Macrotis allows the storage of a statefile either locally or in an AWS S3
bucket (and potentially other remote storage options in the future).  This 
means you can manage as many or as few records as you like in an automated
fashion, as well as having separate automation jobs that manage only their
own particular record sets.

### TinyDNS?
TinyDNS, aka DJBDNS, is a very small DNS resolver daemon with a simple and
straightforward configuration style: Each record has a prefix denoting its
type, and a varying number of additional datapoints, all nicely separated 
by colons.  It's incredibly compact, easy to use, and well-documented.

### Why not Terraform?
Don't get me wrong, Terraform is great.  Unfortunately, the Route53 module
is really only intended to handle, at most, a few dozen DNS records, 
ostensibly attached to whatever other AWS resources are being managed by a
given TF configuration.  Try to use it for bulk zone management and you're 
asking for a world of frustration, `terraform plan` runs that can take
hours upon hours to complete, and silent weeping as you watch every single
record downloaded and compared, one by one by one. It's awful.

### Why not make a replacement TF plugin?
Because Go sucks. Don't @ me.


## Configuration
By default, Macrotis looks for a file called `macrotis.conf` in the working
directory.  It is in JSON format and looks like so:

```json
{
    "provider": {
        "name": (String) A name for the Route53 Provider,
        "region": (String) Region for Route53 Zones,
        "assume_role": (bool) Whether or not to assume a role,
        "role_arn": (String) An IAM ARN for the role to assume
        "session_name": (String) An optional session name
    },
    "statefile": {
        "backend": (String) "s3" or "local",
        "filename": (String) A filename for local statefile storage,
        "bucket": (String) A bucket to store the state in,
        "key": (String) The key within the bucket the file will be stored as,
        "region": (String) Region for S3 bucket
        "tags": {
          (String): (String),
          optional: tags for tagging the S3 bucket/key
          },
        "role_arn": (String) An IAM ARN if a role will be assumed for S3
        "session_name": (String) An optional session name
    },
    "zones": [
        {
            "name": (String) Friendly name for the zone for logging,
            "domain": (String) The domain name for the zone (ie 'domain.com')
            "id": (String) AWS R53 Zone_ID for the zone
        }
    ]
}
```

### Authentication
Macrotis expects you to have set the `AWS_ACCESS_KEY_ID` and 
`AWS_SECRET_ACCESS_KEY` environment variables set.  Or whatever other 
default credential-y things Rusoto supports, I don't know, I didn't read the
docs, I'm a busy person.


## Input file format
As previously noted, Macrotis uses the TinyDNS format for its input files.
Here's an example:

```
+foo.domain.com:1.2.3.4:900
=bar.domain.com:1.2.3.5:900
^6.3.2.1.in-addr.arpa:baz.domain.com:300
```

1. Creates an 'A' record for `foo.domain.com` pointing at `1.2.3.4` with a 
TTL of `900`
2. Creates both an 'A' record for `bar.domain.com` pointing at `1.2.3.5` 
with a TTL of `900` AND a 'PTR' record for `5.3.2.1.in-addr.arpa` pointing
at `bar.domain.com` with a ttl of `900`.
3. Creates a 'PTR' record for `6.3.2.1.in-addr.arpa` pointing at
`baz.domain.com` with a ttl of `300`

Macrotis currently supports all IPv4 TinyDNS record formats.  Nobody uses IPv6.
No you don't, stop lying.

## Requirements
* Rust 1.33
* LibSSL dev libraries installed
* AWS user or role with the following permissions:
  * route53:ChangeResourceRecordSets on `arn:aws:route53:::hostedzone/<zone id>`
  * route53:ListResourceRecordSets on `arn:aws:route53:::hostedzone/<zone id>`
  * s3:GetObject on `arn:aws:s3:::<bucket>`
  * s3:PutObject on `arn:aws:s3:::<bucket>`

##### Last Updated
4-July-2019
