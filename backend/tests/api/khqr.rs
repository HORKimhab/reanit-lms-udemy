//! KHQR encoding, checked against vectors from NBC's official `bakong-khqr` SDK tests.

use chrono::{DateTime, Utc};
use reanit_api::{
    infra::khqr::{self, AccountKind, Khqr, KhqrError, Merchant},
    modules::courses::model::Currency,
};

fn ms(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap()
}

fn individual(account: &str, name: &str) -> Merchant {
    Merchant {
        bakong_account_id: account.into(),
        name: name.into(),
        city: "Phnom Penh".into(),
        kind: AccountKind::Individual {
            account_information: None,
            acquiring_bank: None,
        },
    }
}

fn request(amount_minor: i64, currency: Currency) -> Khqr {
    Khqr {
        amount_minor,
        currency,
        bill_number: None,
        store_label: None,
        terminal_label: None,
        created_at: ms(1_727_249_364_974),
        expires_at: ms(1_727_249_484_974),
    }
}

#[test]
fn encodes_the_sdk_dynamic_individual_vector() {
    let qr = khqr::encode(
        &individual("vandoeurn@devb", "Vandoeurn"),
        &request(10_100, Currency::Usd),
    )
    .unwrap();

    assert_eq!(
        qr,
        "00020101021229180014vandoeurn@devb52045999530384054031015802KH5909Vandoeurn\
         6010Phnom Penh9934001317272493649740113172724948497463047BD0"
    );
}

#[test]
fn crc_matches_sdk_strings() {
    let static_qr =
        "00020101021129180014jonhsmith@nbcq52041A2B53031165802KH5910Jonh Smith6010Phnom Penh6304";
    assert_eq!(khqr::crc16(static_qr), "F7FD");
    let merchant_qr = "00020101021130470009khqr@aclb0111855124649170215ACLEDA Bank Plc5204599953038405802KH\
                       5907BUN MAO6010Phnom Penh6102126213020901050033164310002KM0107BUN MAO0210Phnom Penh6304";
    assert_eq!(khqr::crc16(merchant_qr), "E313");
}

#[test]
fn usd_amounts_use_two_decimals_only_when_needed() {
    let m = individual("dara@abaa", "Dara");
    let qr = khqr::encode(&m, &request(1050, Currency::Usd)).unwrap();
    assert!(qr.contains("540510.50"), "{qr}");
    let qr = khqr::encode(&m, &request(5, Currency::Usd)).unwrap();
    assert!(qr.contains("54040.05"), "{qr}");
}

#[test]
fn khr_amounts_are_whole_riel_with_currency_116() {
    let qr = khqr::encode(
        &individual("dara@abaa", "Dara"),
        &request(40_000, Currency::Khr),
    )
    .unwrap();
    assert!(qr.contains("5303116540540000"), "{qr}");
}

#[test]
fn merchant_accounts_use_tag_30_and_bill_numbers_go_in_tag_62() {
    let merchant = Merchant {
        bakong_account_id: "reanit@abaa".into(),
        name: "ReanIT".into(),
        city: "Phnom Penh".into(),
        kind: AccountKind::Merchant {
            merchant_id: "123456".into(),
            acquiring_bank: "ABA Bank".into(),
        },
    };
    let mut req = request(100, Currency::Usd);
    req.bill_number = Some("ORD12345".into());

    let qr = khqr::encode(&merchant, &req).unwrap();

    assert!(
        qr.starts_with("00020101021230370011reanit@abaa01061234560208ABA Bank"),
        "{qr}"
    );
    assert!(qr.contains("62120108ORD12345"), "{qr}");
    assert_eq!(&qr[qr.len() - 8..qr.len() - 4], "6304");
    assert_eq!(khqr::crc16(&qr[..qr.len() - 4]), qr[qr.len() - 4..]);
}

#[test]
fn rejects_values_the_spec_does_not_allow() {
    let long_name = individual("dara@abaa", "A merchant name that is way too long");
    assert_eq!(
        khqr::encode(&long_name, &request(100, Currency::Usd)),
        Err(KhqrError::TooLong("merchant name"))
    );
    assert_eq!(
        khqr::encode(
            &individual("no-at-sign", "Dara"),
            &request(100, Currency::Usd)
        ),
        Err(KhqrError::InvalidAccountId)
    );
    assert_eq!(
        khqr::encode(&individual("dara@abaa", "Dara"), &request(0, Currency::Usd)),
        Err(KhqrError::InvalidAmount)
    );
    let mut expired = request(100, Currency::Usd);
    expired.expires_at = expired.created_at;
    assert_eq!(
        khqr::encode(&individual("dara@abaa", "Dara"), &expired),
        Err(KhqrError::ExpiryNotAfterCreation)
    );
}

#[test]
fn md5_is_the_lowercase_hex_digest_bakong_looks_up() {
    assert_eq!(khqr::md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
}

#[test]
fn renders_an_svg_qr_code() {
    let svg = khqr::svg("00020101021229180014vandoeurn@devb");
    assert!(
        svg.starts_with("<?xml") || svg.starts_with("<svg"),
        "{}",
        &svg[..40]
    );
    assert!(svg.contains("</svg>"));
}
