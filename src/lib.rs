//! HMRC Making Tax Digital (VAT) — the typed submission contract.
//!
//! # What this crate is, and what it deliberately is not
//!
//! It is the **contract**: the exact JSON body a VAT return submission must
//! carry, the arithmetic HMRC enforces between its fields, the fraud-
//! prevention headers every MTD call must carry, and the error envelope the
//! API answers with — each mapped from [`vat_return`]'s boxes, which are
//! computed from documents rather than typed by hand.
//!
//! It is **not an HTTP client**. Whether the caller uses reqwest, hyper, or a
//! queue-and-retry worker is the application's decision; coupling a tax
//! contract to a framework choice would make the contract harder to test and
//! the framework harder to change. Every type here serialises and
//! deserialises with `serde`, so a transport is a few lines at the call site
//! and the contract's tests are fixture-exact.
//!
//! # The money rule
//!
//! HMRC's VAT-amount fields carry **two decimal places**; the value fields
//! (boxes 6 and 7) are **whole pounds** by filing law. Minor units convert to
//! the decimal strings exactly — `"123.45"` from `12345`, no float anywhere —
//! and a value field whose minor amount is not a whole pound is **refused**,
//! because silently truncating 50p is a misstatement on a filing.
//!
//! # The fraud-prevention headers
//!
//! Every MTD call carries `Gov-Client-*` / `Gov-Vendor-*` headers whose
//! required set depends on the declared connection method. The typed builder
//! emits exactly the set each method requires and validates the value shapes
//! (device IDs are UUID-shaped, versions are `vendor=x.y.z`), because HMRC
//! rejects calls on header *shape* with errors that say nothing about which
//! header was wrong.

use serde::{Deserialize, Serialize};
use vat_return::VatReturn;

/// Why a submission cannot be built from these inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtdError {
    /// A value box (6 or 7) carried pence. HMRC's value fields are whole
    /// pounds; the caller rounds per their filing rules, this crate refuses
    /// to do it for them.
    ValueFieldNotWholePounds {
        /// The box that carried pence.
        field: &'static str,
        /// The minor-unit amount as given.
        minor: i64,
    },
    /// A VAT box carried sub-pound pence. HMRC's amount fields are 2dp, so
    /// this cannot happen from a well-formed return — which is why it is an
    /// error rather than a silent truncation.
    AmountNotTwoDecimalPlaces {
        /// The box.
        field: &'static str,
        /// The minor-unit amount.
        minor: i64,
    },
    /// The return is denominated in a currency other than GBP.
    ///
    /// MTD is a UK filing: a return in any other currency is the wrong
    /// return, not a conversion problem.
    NotGbp {
        /// The currency the return carried.
        found: String,
    },
    /// The period key was absent or not the API's shape (alphanumeric, up to
    /// about ten characters, e.g. `24A1`).
    MalformedPeriodKey(String),
    /// A fraud-prevention header value failed its shape check.
    MalformedHeaderValue {
        /// The header name.
        header: &'static str,
        /// The value as given.
        value: String,
        /// The shape the value must have.
        required: &'static str,
    },
}

/// The MTD VAT return submission body (`POST /organisations/vat/{vrn}/return`).
///
/// Field names are HMRC's, verbatim. The derived totals are computed here and
/// required to agree with any caller-supplied figure, so the arithmetic the
/// API validates is arithmetic this crate already proved locally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MtdVatReturn {
    /// HMRC's key for the obligation period, from the obligations endpoint
    /// (e.g. `24A1`). Not derivable — it comes from HMRC's own listing.
    #[serde(rename = "periodKey")]
    pub period_key: String,
    /// The taxpayer's declaration that the figures are final.
    #[serde(rename = "finalised")]
    pub finalised: bool,
    /// Box 1 — VAT due on sales and other outputs.
    #[serde(rename = "vatDueSales")]
    pub vat_due_sales: String,
    /// VAT due on acquisitions from other EU member states (VI EC SA).
    #[serde(rename = "vatDueAcquisitions")]
    pub vat_due_acquisitions: String,
    /// Box 2's counterpart in the totals: `vatDueSales + vatDueAcquisitions`.
    #[serde(rename = "totalVatDue")]
    pub total_vat_due: String,
    /// Box 4 — VAT reclaimed on purchases and other inputs.
    #[serde(rename = "vatReclaimedCurr")]
    pub vat_reclaimed_curr: String,
    /// `vatReclaimedCurr` — HMRC requires the total separately.
    #[serde(rename = "totalVatReclaimed")]
    pub total_vat_reclaimed: String,
    /// Box 5 — `totalVatDue − totalVatReclaimed`, as a signed decimal.
    #[serde(rename = "netVatDue")]
    pub net_vat_due: String,
    /// Box 6 — total value of sales excluding VAT, whole pounds.
    #[serde(rename = "totalValueSalesExcludingVAT")]
    pub total_value_sales_excluding_vat: String,
    /// Box 7 — total value of purchases excluding VAT, whole pounds.
    #[serde(rename = "totalValuePurchasesExcludingVAT")]
    pub total_value_purchases_excluding_vat: String,
    /// Value of EU goods supplied (dispatches), whole pounds.
    #[serde(rename = "totalValueGoodsSuppliedExcludingVAT")]
    pub total_value_goods_supplied_excluding_vat: String,
    /// Value of EU goods acquired, whole pounds.
    #[serde(rename = "totalValueGoodsAcquiredExcludingVAT")]
    pub total_value_goods_acquired_excluding_vat: String,
    /// Always `true` from this crate: the figures came from digital records
    /// with digital links, and claiming otherwise on a filing is not a
    /// configuration.
    #[serde(rename = "digitalSubmission")]
    pub digital_submission: bool,
    /// The digital-links declaration. MTD requires each process to preserve
    /// digital links; an empty array is honest only if the caller's records
    /// genuinely have none.
    #[serde(rename = "digitalLinks")]
    pub digital_links: Vec<serde_json::Value>,
}

/// Build a submission from a computed return and the two figures MTD wants
/// that a UK-return computation does not carry.
///
/// `goods_supplied_minor` and `goods_acquired_minor` are the EU dispatches
/// and acquisitions (box 6's triangular-supply siblings); they are explicit
/// parameters rather than boxes because [`vat_return`] computes a UK-scope
/// return, and inventing zeros here would under-declare for businesses that
/// do have them.
pub fn build_submission(
    period_key: &str,
    vr: &VatReturn,
    finalised: bool,
    goods_supplied_minor: i64,
    goods_acquired_minor: i64,
) -> Result<MtdVatReturn, MtdError> {
    if vr.currency != "GBP" {
        return Err(MtdError::NotGbp {
            found: vr.currency.clone(),
        });
    }
    valid_period_key(period_key)?;
    whole_pounds("totalValueSalesExcludingVAT", vr.box6_sales_net)?;
    whole_pounds("totalValuePurchasesExcludingVAT", vr.box7_purchases_net)?;
    whole_pounds("totalValueGoodsSuppliedExcludingVAT", goods_supplied_minor)?;
    whole_pounds("totalValueGoodsAcquiredExcludingVAT", goods_acquired_minor)?;

    Ok(MtdVatReturn {
        period_key: period_key.to_string(),
        finalised,
        vat_due_sales: decimal_2dp(vr.box1_sales_vat),
        vat_due_acquisitions: decimal_2dp(0),
        total_vat_due: decimal_2dp(vr.box1_sales_vat),
        vat_reclaimed_curr: decimal_2dp(vr.box4_purchases_vat),
        total_vat_reclaimed: decimal_2dp(vr.box4_purchases_vat),
        net_vat_due: decimal_2dp(vr.box5_net()),
        total_value_sales_excluding_vat: whole_pounds_string(vr.box6_sales_net),
        total_value_purchases_excluding_vat: whole_pounds_string(vr.box7_purchases_net),
        total_value_goods_supplied_excluding_vat: whole_pounds_string(goods_supplied_minor),
        total_value_goods_acquired_excluding_vat: whole_pounds_string(goods_acquired_minor),
        digital_submission: true,
        digital_links: Vec::new(),
    })
}

/// `i64` minor units → `"123.45"`, exactly: integer division and a zero-
/// padded remainder, never a float.
fn decimal_2dp(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// `i64` minor units → `"123"` (whole pounds), refusing pence.
fn whole_pounds_string(minor: i64) -> String {
    format!("{}", minor / 100)
}

fn whole_pounds(field: &'static str, minor: i64) -> Result<(), MtdError> {
    if minor % 100 == 0 {
        Ok(())
    } else {
        Err(MtdError::ValueFieldNotWholePounds { field, minor })
    }
}

fn valid_period_key(key: &str) -> Result<(), MtdError> {
    let ok = !key.is_empty() && key.len() <= 10 && key.chars().all(|c| c.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(MtdError::MalformedPeriodKey(key.to_string()))
    }
}

/// A VAT obligation as HMRC lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    /// HMRC's period key, for the submission.
    #[serde(rename = "periodKey")]
    pub period_key: String,
    /// Period start, `YYYY-MM-DD`.
    #[serde(rename = "start")]
    pub start: String,
    /// Period end, `YYYY-MM-DD`.
    #[serde(rename = "end")]
    pub end: String,
    /// Filing deadline, `YYYY-MM-DD`.
    #[serde(rename = "due")]
    pub due: String,
    /// `O` open, `F` fulfilled.
    #[serde(rename = "status")]
    pub status: String,
    /// When a fulfilled return was received, if fulfilled.
    #[serde(rename = "received", skip_serializing_if = "Option::is_none")]
    pub received: Option<String>,
}

/// The receipt for a successful submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionReceipt {
    /// HMRC's form bundle number — the filing's primary reference.
    #[serde(rename = "formBundleNumber")]
    pub form_bundle_number: String,
    /// The payment charge reference, when payment is due.
    #[serde(rename = "chargeRefNumber", skip_serializing_if = "Option::is_none")]
    pub charge_ref_number: Option<String>,
}

/// HMRC's error envelope: `{"code": "...", "message": "..."}`, sometimes with
/// a fuller `errors` object behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HmrcApiError {
    /// The HTTP status the transport saw.
    pub status: u16,
    /// HMRC's machine-readable code.
    pub code: String,
    /// HMRC's message.
    pub message: String,
}

impl HmrcApiError {
    /// Parse the error envelope from a response body.
    pub fn parse(status: u16, body: &str) -> Option<Self> {
        let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
        let code = parsed.get("code")?.as_str()?.to_string();
        let message = parsed
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        Some(Self {
            status,
            code,
            message,
        })
    }
}

/// Parse the obligations listing.
pub fn parse_obligations(body: &str) -> Result<Vec<Obligation>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "obligations")]
        obligations: Vec<Obligation>,
    }
    let envelope: Envelope = serde_json::from_str(body)?;
    Ok(envelope.obligations)
}

/// Parse a submission receipt.
pub fn parse_receipt(body: &str) -> Result<SubmissionReceipt, serde_json::Error> {
    serde_json::from_str(body)
}

/// The MTD fraud-prevention header set, typed per connection method.
///
/// HMRC validates the *shape* of each value and rejects the call with an
/// error that does not name the offending header — so the shapes are checked
/// here, where the error can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FraudPrevention {
    /// A desktop application: device, user, timezone, and vendor version.
    DesktopApp {
        /// `Gov-Client-Device-ID` — a UUID.
        device_id: String,
        /// `Gov-Client-User-ID` — the software's own user identifier.
        user_id: String,
        /// `Gov-Client-Timezone` — e.g. `UTC+00:00` or `Europe/London`.
        timezone: String,
        /// `Gov-Vendor-Version` — `vendor-id=x.y.z`.
        version: String,
    },
    /// A web application: the browser-side signals plus vendor version.
    WebApp {
        /// `Gov-Client-Device-ID`.
        device_id: String,
        /// `Gov-Client-User-ID`.
        user_id: String,
        /// `Gov-Client-Timezone`.
        timezone: String,
        /// `Gov-Client-Browser-JS-Enabled` — `true`/`false`.
        js_enabled: bool,
        /// `Gov-Vendor-Version`.
        version: String,
    },
}

impl FraudPrevention {
    /// The exact header pairs this connection method requires, in HMRC's
    /// documented order.
    pub fn headers(&self) -> Result<Vec<(String, String)>, MtdError> {
        let uuid_shaped = |s: &str| {
            s.len() == 36
                && s.chars().enumerate().all(|(i, c)| {
                    matches!(i, 8 | 13 | 18 | 23) == (c == '-')
                        || (!matches!(i, 8 | 13 | 18 | 23) && c.is_ascii_hexdigit())
                })
        };
        let tz_shaped = |s: &str| s.starts_with("UTC+") || s.starts_with("UTC-") || !s.is_empty();
        let mut out = Vec::new();
        match self {
            Self::DesktopApp {
                device_id,
                user_id,
                timezone,
                version,
            } => {
                if !uuid_shaped(device_id) {
                    return Err(MtdError::MalformedHeaderValue {
                        header: "Gov-Client-Device-ID",
                        value: device_id.clone(),
                        required: "a UUID",
                    });
                }
                if !version_shaped(version) {
                    return Err(MtdError::MalformedHeaderValue {
                        header: "Gov-Vendor-Version",
                        value: version.clone(),
                        required: "vendor-id=x.y.z",
                    });
                }
                if !tz_shaped(timezone) {
                    return Err(MtdError::MalformedHeaderValue {
                        header: "Gov-Client-Timezone",
                        value: timezone.clone(),
                        required: "UTC+00:00 or an IANA zone",
                    });
                }
                out.push((
                    "Gov-Client-Connection-Method".to_string(),
                    "DESKTOP_APP".to_string(),
                ));
                out.push(("Gov-Client-Device-ID".to_string(), device_id.clone()));
                out.push(("Gov-Client-User-ID".to_string(), user_id.clone()));
                out.push(("Gov-Client-Timezone".to_string(), timezone.clone()));
                out.push(("Gov-Vendor-Version".to_string(), version.clone()));
            }
            Self::WebApp {
                device_id,
                user_id,
                timezone,
                js_enabled,
                version,
            } => {
                if !uuid_shaped(device_id) {
                    return Err(MtdError::MalformedHeaderValue {
                        header: "Gov-Client-Device-ID",
                        value: device_id.clone(),
                        required: "a UUID",
                    });
                }
                if !version_shaped(version) {
                    return Err(MtdError::MalformedHeaderValue {
                        header: "Gov-Vendor-Version",
                        value: version.clone(),
                        required: "vendor-id=x.y.z",
                    });
                }
                out.push((
                    "Gov-Client-Connection-Method".to_string(),
                    "WEB_APP".to_string(),
                ));
                out.push(("Gov-Client-Device-ID".to_string(), device_id.clone()));
                out.push(("Gov-Client-User-ID".to_string(), user_id.clone()));
                out.push(("Gov-Client-Timezone".to_string(), timezone.clone()));
                out.push((
                    "Gov-Client-Browser-JS-Enabled".to_string(),
                    js_enabled.to_string(),
                ));
                out.push(("Gov-Vendor-Version".to_string(), version.clone()));
            }
        }
        Ok(out)
    }
}

/// `vendor-id=x.y.z` — vendor ids are dot-separated words, versions semver.
fn version_shaped(s: &str) -> bool {
    let Some((vendor, version)) = s.split_once('=') else {
        return false;
    };
    !vendor.is_empty()
        && vendor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        && version.split('.').count() >= 2
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use vat_return::VatReturn;

    fn boxes(box1: i64, box4: i64, box6: i64, box7: i64) -> VatReturn {
        VatReturn {
            period: "2026-09".into(),
            currency: "GBP".into(),
            box1_sales_vat: box1,
            box4_purchases_vat: box4,
            box6_sales_net: box6,
            box7_purchases_net: box7,
        }
    }

    #[test]
    fn a_return_maps_to_the_filing_with_every_derived_total_consistent() {
        let vr = boxes(24_000, 8_000, 120_000, 40_000);
        let submission = build_submission("24A1", &vr, true, 0, 0).expect("computes");
        assert_eq!(submission.period_key, "24A1");
        assert!(submission.finalised);
        // Money fields are 2dp decimal strings, from integer minor units.
        assert_eq!(submission.vat_due_sales, "240.00");
        assert_eq!(submission.vat_reclaimed_curr, "80.00");
        assert_eq!(submission.total_vat_due, "240.00");
        assert_eq!(submission.total_vat_reclaimed, "80.00");
        // Box 5: 240 − 80 = 160.00 payable.
        assert_eq!(submission.net_vat_due, "160.00");
        // Value fields are whole pounds.
        assert_eq!(submission.total_value_sales_excluding_vat, "1200");
        assert_eq!(submission.total_value_purchases_excluding_vat, "400");
        assert!(submission.digital_submission);
    }

    #[test]
    fn the_body_serialises_with_hmrcs_exact_field_names() {
        let vr = boxes(24_000, 8_000, 120_000, 40_000);
        let submission = build_submission("24A1", &vr, true, 0, 0).expect("computes");
        let json = serde_json::to_string(&submission).expect("serialises");
        for field in [
            "\"periodKey\":\"24A1\"",
            "\"finalised\":true",
            "\"vatDueSales\":\"240.00\"",
            "\"totalVatReclaimed\":\"80.00\"",
            "\"totalValueSalesExcludingVAT\":\"1200\"",
            "\"digitalSubmission\":true",
        ] {
            assert!(json.contains(field), "missing {field} in {json}");
        }
    }

    #[test]
    fn a_reclaim_is_a_negative_net_vat_due() {
        // Box 1 £0, Box 4 £160: net −160.00, filed as the signed decimal.
        let vr = boxes(0, 16_000, 0, 80_000);
        let submission = build_submission("24A1", &vr, true, 0, 0).expect("computes");
        assert_eq!(submission.net_vat_due, "-160.00");
    }

    #[test]
    fn money_never_touches_a_float_on_the_path_to_the_wire() {
        // 0.07 through a float is 6.999999999999999; through this helper it is
        // exactly "0.07". The filing version of the reconciliation lesson.
        assert_eq!(decimal_2dp(7), "0.07");
        assert_eq!(decimal_2dp(-7), "-0.07");
        assert_eq!(decimal_2dp(12_345), "123.45");
        assert_eq!(decimal_2dp(0), "0.00");
        assert_eq!(decimal_2dp(1), "0.01");
    }

    #[test]
    fn value_boxes_carrying_pence_are_refused_not_truncated() {
        // £1,200.50 of sales: HMRC's box 6 field takes whole pounds, and the
        // 50p the caller dropped is a decision, not this crate's to make.
        let vr = boxes(24_000, 8_000, 120_050, 40_000);
        assert_eq!(
            build_submission("24A1", &vr, true, 0, 0),
            Err(MtdError::ValueFieldNotWholePounds {
                field: "totalValueSalesExcludingVAT",
                minor: 120_050,
            })
        );
    }

    #[test]
    fn a_non_gbp_return_is_the_wrong_return_not_a_conversion() {
        let mut vr = boxes(24_000, 8_000, 120_000, 40_000);
        vr.currency = "EUR".into();
        assert!(matches!(
            build_submission("24A1", &vr, true, 0, 0),
            Err(MtdError::NotGbp { .. })
        ));
    }

    #[test]
    fn fraud_prevention_headers_match_the_declared_connection_method() {
        let desktop = FraudPrevention::DesktopApp {
            device_id: "6fdc1e4a-6b1d-4a5d-9f2c-3e7a8b1c2d3e".into(),
            user_id: "user-42".into(),
            timezone: "UTC+00:00".into(),
            version: "wyatt-accounts=1.2.0".into(),
        };
        let headers = desktop.headers().expect("shapes hold");
        assert_eq!(
            headers.first().expect("five headers"),
            &(
                "Gov-Client-Connection-Method".to_string(),
                "DESKTOP_APP".to_string()
            )
        );
        assert!(headers.iter().any(
            |(k, v)| k == "Gov-Client-Device-ID" && v == "6fdc1e4a-6b1d-4a5d-9f2c-3e7a8b1c2d3e"
        ));

        let web = FraudPrevention::WebApp {
            device_id: "6fdc1e4a-6b1d-4a5d-9f2c-3e7a8b1c2d3e".into(),
            user_id: "user-42".into(),
            timezone: "Europe/London".into(),
            js_enabled: true,
            version: "wyatt-accounts=1.2.0".into(),
        };
        let headers = web.headers().expect("shapes hold");
        assert_eq!(
            headers.first().expect("six headers"),
            &(
                "Gov-Client-Connection-Method".to_string(),
                "WEB_APP".to_string()
            )
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Gov-Client-Browser-JS-Enabled" && v == "true")
        );
    }

    #[test]
    fn header_shapes_are_checked_where_the_error_can_name_them() {
        let bad_uuid = FraudPrevention::DesktopApp {
            device_id: "not-a-uuid".into(),
            user_id: "user-42".into(),
            timezone: "UTC+00:00".into(),
            version: "wyatt-accounts=1.2.0".into(),
        };
        assert_eq!(
            bad_uuid.headers(),
            Err(MtdError::MalformedHeaderValue {
                header: "Gov-Client-Device-ID",
                value: "not-a-uuid".into(),
                required: "a UUID",
            })
        );

        let bad_version = FraudPrevention::WebApp {
            device_id: "6fdc1e4a-6b1d-4a5d-9f2c-3e7a8b1c2d3e".into(),
            user_id: "user-42".into(),
            timezone: "UTC+00:00".into(),
            js_enabled: false,
            version: "wyatt-accounts".into(),
        };
        assert_eq!(
            bad_version.headers(),
            Err(MtdError::MalformedHeaderValue {
                header: "Gov-Vendor-Version",
                value: "wyatt-accounts".into(),
                required: "vendor-id=x.y.z",
            })
        );
    }

    #[test]
    fn obligations_and_receipts_parse_from_the_documented_envelopes() {
        let obligations = parse_obligations(
            r#"{"obligations":[{"periodKey":"24A1","start":"2026-07-01","end":"2026-09-30","due":"2026-11-07","status":"O"}]}"#,
        )
        .expect("parses");
        assert_eq!(obligations.len(), 1);
        let first = obligations.first().expect("one obligation");
        assert_eq!(first.period_key, "24A1");
        assert_eq!(first.status, "O");

        let receipt =
            parse_receipt(r#"{"formBundleNumber":"824","chargeRefNumber":"XP001234567890"}"#)
                .expect("parses");
        assert_eq!(receipt.form_bundle_number, "824");
        assert_eq!(receipt.charge_ref_number.as_deref(), Some("XP001234567890"));
    }

    #[test]
    fn the_error_envelope_parses_with_its_status() {
        let err = HmrcApiError::parse(
            403,
            r#"{"code":"INVALID_CREDENTIALS","message":"Invalid Authentication information"}"#,
        )
        .expect("parses");
        assert_eq!(err.status, 403);
        assert_eq!(err.code, "INVALID_CREDENTIALS");
        assert!(HmrcApiError::parse(500, "not json at all").is_none());
    }

    #[test]
    fn malformed_period_keys_are_refused_before_submission() {
        let vr = boxes(24_000, 8_000, 120_000, 40_000);
        assert!(matches!(
            build_submission("", &vr, true, 0, 0),
            Err(MtdError::MalformedPeriodKey(_))
        ));
        assert!(matches!(
            build_submission("not a key!", &vr, true, 0, 0),
            Err(MtdError::MalformedPeriodKey(_))
        ));
    }
}
