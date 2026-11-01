//! Dynamic KHQR (Cambodia's EMVCo-based QR standard) encoding.
//!
//! Field layout, lengths and the CRC follow NBC's official `bakong-khqr` SDK:
//! tags 00, 01, 29|30, 52, 53, 54, 58, 59, 60, [62], 99, 63 in that order,
//! each as `ID + 2-digit length + value`, and CRC-16/CCITT-FALSE over the
//! whole string including the `6304` CRC header.

use chrono::{DateTime, Utc};
use md5::{Digest, Md5};

use crate::modules::courses::model::Currency;

const DEFAULT_MERCHANT_CATEGORY_CODE: &str = "5999";

#[derive(Clone, Debug)]
pub enum AccountKind {
    /// Tag 29: a personal Bakong account.
    Individual {
        account_information: Option<String>,
        acquiring_bank: Option<String>,
    },
    /// Tag 30: a merchant account registered with an acquiring bank.
    Merchant {
        merchant_id: String,
        acquiring_bank: String,
    },
}

/// Who gets paid.
#[derive(Clone, Debug)]
pub struct Merchant {
    /// e.g. `reanit@abaa`.
    pub bakong_account_id: String,
    pub name: String,
    pub city: String,
    pub kind: AccountKind,
}

/// One payment request.
#[derive(Clone, Debug)]
pub struct Khqr {
    /// USD cents or whole riel.
    pub amount_minor: i64,
    pub currency: Currency,
    pub bill_number: Option<String>,
    pub store_label: Option<String>,
    pub terminal_label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KhqrError {
    #[error("{0} is too long for KHQR")]
    TooLong(&'static str),
    #[error("{0} must be plain ASCII for KHQR")]
    NotAscii(&'static str),
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("Bakong account id must look like name@bank")]
    InvalidAccountId,
    #[error("amount must be positive")]
    InvalidAmount,
    #[error("expiry must be after creation")]
    ExpiryNotAfterCreation,
}

/// `ID + LL + value`, validating length and charset.
fn tlv(
    out: &mut String,
    id: &str,
    value: &str,
    max: usize,
    field: &'static str,
) -> Result<(), KhqrError> {
    if value.is_empty() {
        return Err(KhqrError::Missing(field));
    }
    if !value.is_ascii() {
        return Err(KhqrError::NotAscii(field));
    }
    if value.len() > max {
        return Err(KhqrError::TooLong(field));
    }
    out.push_str(id);
    out.push_str(&format!("{:02}", value.len()));
    out.push_str(value);
    Ok(())
}

/// USD: `10`, `10.50`, `0.05` (decimals only when needed, as the SDK does). KHR: whole riel.
fn format_amount(amount_minor: i64, currency: Currency) -> String {
    match currency {
        Currency::Khr => amount_minor.to_string(),
        Currency::Usd if amount_minor % 100 == 0 => (amount_minor / 100).to_string(),
        Currency::Usd => format!("{}.{:02}", amount_minor / 100, amount_minor % 100),
    }
}

fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Usd => "840",
        Currency::Khr => "116",
    }
}

/// Builds a dynamic (amount-bearing, expiring) KHQR string.
pub fn encode(merchant: &Merchant, req: &Khqr) -> Result<String, KhqrError> {
    if req.amount_minor <= 0 {
        return Err(KhqrError::InvalidAmount);
    }
    if req.expires_at <= req.created_at {
        return Err(KhqrError::ExpiryNotAfterCreation);
    }
    let account = &merchant.bakong_account_id;
    if account.split('@').filter(|p| !p.is_empty()).count() < 2 {
        return Err(KhqrError::InvalidAccountId);
    }

    let mut qr = String::with_capacity(256);
    qr.push_str("000201"); // payload format indicator
    qr.push_str("010212"); // point of initiation: dynamic

    let mut account_info = String::new();
    tlv(&mut account_info, "00", account, 32, "Bakong account id")?;
    let account_tag = match &merchant.kind {
        AccountKind::Individual {
            account_information,
            acquiring_bank,
        } => {
            if let Some(info) = account_information {
                tlv(&mut account_info, "01", info, 32, "account information")?;
            }
            if let Some(bank) = acquiring_bank {
                tlv(&mut account_info, "02", bank, 32, "acquiring bank")?;
            }
            "29"
        }
        AccountKind::Merchant {
            merchant_id,
            acquiring_bank,
        } => {
            tlv(&mut account_info, "01", merchant_id, 32, "merchant id")?;
            tlv(
                &mut account_info,
                "02",
                acquiring_bank,
                32,
                "acquiring bank",
            )?;
            "30"
        }
    };
    tlv(
        &mut qr,
        account_tag,
        &account_info,
        99,
        "account information",
    )?;

    tlv(
        &mut qr,
        "52",
        DEFAULT_MERCHANT_CATEGORY_CODE,
        4,
        "merchant category code",
    )?;
    tlv(&mut qr, "53", currency_code(req.currency), 3, "currency")?;
    tlv(
        &mut qr,
        "54",
        &format_amount(req.amount_minor, req.currency),
        13,
        "amount",
    )?;
    tlv(&mut qr, "58", "KH", 2, "country code")?;
    tlv(&mut qr, "59", &merchant.name, 25, "merchant name")?;
    tlv(&mut qr, "60", &merchant.city, 15, "merchant city")?;

    let mut additional = String::new();
    if let Some(bill) = &req.bill_number {
        tlv(&mut additional, "01", bill, 25, "bill number")?;
    }
    if let Some(store) = &req.store_label {
        tlv(&mut additional, "03", store, 25, "store label")?;
    }
    if let Some(terminal) = &req.terminal_label {
        tlv(&mut additional, "07", terminal, 25, "terminal label")?;
    }
    if !additional.is_empty() {
        tlv(&mut qr, "62", &additional, 99, "additional data")?;
    }

    let mut timestamps = String::new();
    tlv(
        &mut timestamps,
        "00",
        &req.created_at.timestamp_millis().to_string(),
        13,
        "creation timestamp",
    )?;
    tlv(
        &mut timestamps,
        "01",
        &req.expires_at.timestamp_millis().to_string(),
        13,
        "expiration timestamp",
    )?;
    tlv(&mut qr, "99", &timestamps, 99, "timestamps")?;

    qr.push_str("6304");
    let crc = crc16(&qr);
    qr.push_str(&crc);
    Ok(qr)
}

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF), 4 uppercase hex digits.
pub fn crc16(data: &str) -> String {
    let mut crc: u16 = 0xFFFF;
    for byte in data.bytes() {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    format!("{crc:04X}")
}

/// Lowercase hex MD5: Bakong looks transactions up by the MD5 of the QR string.
pub fn md5_hex(data: &str) -> String {
    hex::encode(Md5::digest(data.as_bytes()))
}

/// The QR code as an SVG document.
pub fn svg(data: &str) -> String {
    use qrcode::{EcLevel, QrCode, render::svg};
    // The payload is ASCII and well under QR capacity, so this cannot fail.
    let code = QrCode::with_error_correction_level(data, EcLevel::M).expect("KHQR fits a QR code");
    code.render::<svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build()
}
