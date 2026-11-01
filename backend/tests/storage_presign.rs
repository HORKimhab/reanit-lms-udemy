//! The SigV4 presigner, checked against the worked example in the AWS docs
//! ("Authenticating Requests: Using Query Parameters (AWS Signature Version 4)").

use chrono::{TimeZone, Utc};
use reanit_api::infra::storage::{PresignRequest, presign_url};

#[test]
fn matches_the_aws_documentation_example() {
    let url = presign_url(&PresignRequest {
        method: "GET",
        scheme: "https",
        host: "examplebucket.s3.amazonaws.com",
        path: "/test.txt",
        region: "us-east-1",
        access_key: "AKIAIOSFODNN7EXAMPLE",
        secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        now: Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap(),
        expires_secs: 86400,
    });

    assert_eq!(
        url,
        "https://examplebucket.s3.amazonaws.com/test.txt\
         ?X-Amz-Algorithm=AWS4-HMAC-SHA256\
         &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
         &X-Amz-Date=20130524T000000Z\
         &X-Amz-Expires=86400\
         &X-Amz-SignedHeaders=host\
         &X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
    );
}
