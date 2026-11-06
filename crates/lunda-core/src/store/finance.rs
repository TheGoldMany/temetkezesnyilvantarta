//! Finance and cash desk (W7, docs/FRS-UZEMELTETES.md FR-PEN-01..10):
//! charges from the fee decree, invoices through the operator's own
//! Számlázz.hu account or prepared for the municipal ASP, payments, a cash
//! desk with gap-free receipt numbers and closed days, bank statement
//! matching, receivables and revenue reports.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::settings;
use crate::sql::{params, params_from_iter, OptionalExtension, Value as SqlValue};
use crate::szamlazz::{self, Document, Line, Party};
use crate::util::{fmt_date, new_id, now_ts, parse_date, sha256_hex, today};

/// Accepted VAT codes: percentages and the Számlázz.hu codes for exempt items.
pub const VAT_CODES: &[&str] = &["27", "18", "5", "0", "AAM", "TAM", "EUFAD37", "KBAET", "ATK"];

pub fn check_vat(v: &str) -> Result<String> {
    let v = v.trim();
    let v = if v.is_empty() { "27" } else { v };
    if VAT_CODES.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(CoreError::validation("vatRate", format!("unknown VAT code {v}")).with("code", "vat_invalid"))
    }
}

/// VAT on a net amount; exempt codes carry no VAT.
pub fn vat_of(net: i64, rate: &str) -> i64 {
    match rate.parse::<i64>() {
        Ok(p) => (net as f64 * p as f64 / 100.0).round() as i64,
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------- types

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChargeInput {
    pub cemetery_id: String,
    pub person_id: Option<String>,
    pub payer_name: Option<String>,
    pub fee_rate_id: Option<String>,
    pub description: Option<String>,
    pub quantity: Option<i64>,
    pub unit: Option<String>,
    /// overrides the fee item's price (needs `override_reason`)
    pub unit_net_huf: Option<i64>,
    pub vat_rate: Option<String>,
    pub ledger_code: Option<String>,
    pub entity: Option<String>,
    pub entity_id: Option<String>,
    pub override_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Charge {
    pub id: String,
    pub cemetery_id: String,
    pub person_id: Option<String>,
    pub payer_name: String,
    pub fee_rate_id: Option<String>,
    pub description: String,
    pub quantity: i64,
    pub unit: String,
    pub unit_net_huf: i64,
    pub vat_rate: String,
    pub net_huf: i64,
    pub vat_huf: i64,
    pub gross_huf: i64,
    pub ledger_code: String,
    pub entity: Option<String>,
    pub entity_id: Option<String>,
    pub status: String,
    pub invoice_id: Option<String>,
    pub override_reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceInput {
    pub cemetery_id: String,
    pub charge_ids: Vec<String>,
    pub person_id: Option<String>,
    pub buyer_name: String,
    pub buyer_postcode: String,
    pub buyer_city: String,
    pub buyer_address: String,
    #[serde(default)]
    pub buyer_tax_number: String,
    #[serde(default)]
    pub buyer_email: String,
    /// cash | transfer | card
    pub payment_method: String,
    pub fulfilment_date: Option<String>,
    pub due_days: Option<i64>,
    #[serde(default)]
    pub proforma: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: String,
    pub cemetery_id: String,
    pub kind: String,
    pub provider: String,
    pub number: String,
    pub storno_of: Option<String>,
    pub person_id: Option<String>,
    pub buyer_name: String,
    pub buyer_address: String,
    pub buyer_tax_number: String,
    pub buyer_email: String,
    pub issue_date: String,
    pub fulfilment_date: String,
    pub due_date: String,
    pub payment_method: String,
    pub net_huf: i64,
    pub vat_huf: i64,
    pub gross_huf: i64,
    pub paid_huf: i64,
    pub status: String,
    pub has_pdf: bool,
    pub days_overdue: Option<i64>,
    pub reminders: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInput {
    pub cemetery_id: String,
    pub invoice_id: Option<String>,
    pub person_id: Option<String>,
    pub payer_name: Option<String>,
    pub amount_huf: i64,
    pub method: String,
    pub paid_on: Option<String>,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CashEntry {
    pub id: String,
    pub cemetery_id: String,
    pub entry_date: String,
    pub kind: String,
    pub doc_number: String,
    pub amount_huf: i64,
    pub partner_name: String,
    pub description: String,
    pub payment_id: Option<String>,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BankTx {
    pub id: String,
    pub booked_on: String,
    pub amount_huf: i64,
    pub partner: String,
    pub reference: String,
    pub status: String,
    pub payment_id: Option<String>,
    /// best guess for an open invoice (number in the reference, or same amount and name)
    pub suggested_invoice_id: Option<String>,
    pub suggested_invoice_number: Option<String>,
}

const CHARGE_COLS: &str = "id, cemetery_id, person_id, payer_name, fee_rate_id, description, quantity, unit, unit_net_huf, vat_rate,
    net_huf, vat_huf, gross_huf, ledger_code, entity, entity_id, status, invoice_id, override_reason, created_at";

fn map_charge(r: &crate::sql::Row) -> crate::sql::Result<Charge> {
    Ok(Charge {
        id: r.get(0)?,
        cemetery_id: r.get(1)?,
        person_id: r.get(2)?,
        payer_name: r.get(3)?,
        fee_rate_id: r.get(4)?,
        description: r.get(5)?,
        quantity: r.get(6)?,
        unit: r.get(7)?,
        unit_net_huf: r.get(8)?,
        vat_rate: r.get(9)?,
        net_huf: r.get(10)?,
        vat_huf: r.get(11)?,
        gross_huf: r.get(12)?,
        ledger_code: r.get(13)?,
        entity: r.get(14)?,
        entity_id: r.get(15)?,
        status: r.get(16)?,
        invoice_id: r.get(17)?,
        override_reason: r.get(18)?,
        created_at: r.get(19)?,
    })
}

const INVOICE_COLS: &str = "i.id, i.cemetery_id, i.kind, i.provider, i.number, i.storno_of, i.person_id, i.buyer_name, i.buyer_address, i.buyer_tax_number,
    i.buyer_email, i.issue_date, i.fulfilment_date, i.due_date, i.payment_method, i.net_huf, i.vat_huf, i.gross_huf, i.paid_huf, i.status,
    i.pdf IS NOT NULL, (SELECT count(*) FROM reminders m WHERE m.invoice_id = i.id)";

fn map_invoice(r: &crate::sql::Row) -> crate::sql::Result<Invoice> {
    let due: String = r.get(13)?;
    let status: String = r.get(19)?;
    let overdue =
        crate::util::parse_date(&due).ok().map(|d| (today() - d).num_days()).filter(|d| *d > 0 && matches!(status.as_str(), "issued" | "partially_paid"));
    Ok(Invoice {
        id: r.get(0)?,
        cemetery_id: r.get(1)?,
        kind: r.get(2)?,
        provider: r.get(3)?,
        number: r.get(4)?,
        storno_of: r.get(5)?,
        person_id: r.get(6)?,
        buyer_name: r.get(7)?,
        buyer_address: r.get(8)?,
        buyer_tax_number: r.get(9)?,
        buyer_email: r.get(10)?,
        issue_date: r.get(11)?,
        fulfilment_date: r.get(12)?,
        due_date: due,
        payment_method: r.get(14)?,
        net_huf: r.get(15)?,
        vat_huf: r.get(16)?,
        gross_huf: r.get(17)?,
        paid_huf: r.get(18)?,
        status,
        has_pdf: r.get(20)?,
        days_overdue: overdue,
        reminders: r.get(21)?,
    })
}

fn person_name(conn: &crate::sql::Connection, id: &str) -> Result<String> {
    Ok(conn.query_row("SELECT trim(name_prefix || ' ' || family_name || ' ' || given_name) FROM persons WHERE id = ?1", params![id], |r| r.get(0))?)
}

fn fee_kind_name(kind: &str) -> &str {
    match kind {
        "purchase" => "Sírhely megváltása",
        "renewal" => "Sírhely újraváltása",
        "opening" => "Sírnyitás",
        "chapel" => "Ravatalozó használata",
        "maintenance" => "Temetőfenntartási hozzájárulás",
        "cooling" => "Hűtés",
        "monument_permit" => "Síremlék-állítási engedély",
        other => other,
    }
}

impl Store {
    // ---------------------------------------------------------------- charges (FR-PEN-02)

    pub fn add_charge(&self, s: &Session, c: &ChargeInput) -> Result<Charge> {
        s.require(Permission::RecordPayments)?;
        non_empty("cemeteryId", &c.cemetery_id)?;
        let rate = match &c.fee_rate_id {
            Some(id) if !id.is_empty() => Some(
                self.conn
                    .query_row(
                        "SELECT id, cemetery_id, plot_type_id, fee_kind, amount_huf, valid_from, valid_to, voided_at, name, unit, vat_rate, ledger_code FROM fee_rates WHERE id = ?1",
                        params![id],
                        crate::store::leases::map_fee_rate,
                    )
                    .optional()?
                    .ok_or_else(|| CoreError::not_found("fee_rate", id))?,
            ),
            _ => None,
        };
        let quantity = c.quantity.unwrap_or(1);
        if quantity < 1 {
            return Err(CoreError::validation("quantity", "quantity must be at least 1"));
        }
        let unit_net = match (c.unit_net_huf, &rate) {
            (Some(p), Some(r)) if p != r.amount_huf => {
                // FR-PEN-02: a changed price needs a reason, kept in the log
                require_reason(c.override_reason.as_deref())?;
                p
            }
            (Some(p), _) => p,
            (None, Some(r)) => r.amount_huf,
            (None, None) => return Err(CoreError::validation("unitNetHuf", "price required").with("code", "required")),
        };
        if unit_net < 0 {
            return Err(CoreError::validation("unitNetHuf", "price must not be negative"));
        }
        let description = opt_s(&c.description)
            .or_else(|| rate.as_ref().map(|r| if r.name.trim().is_empty() { fee_kind_name(&r.fee_kind).to_string() } else { r.name.clone() }))
            .ok_or_else(|| CoreError::validation("description", "description required").with("code", "required"))?;
        let default_vat = settings::get(&self.conn, "finance.default_vat", None)?.unwrap_or_else(|| "27".into());
        let vat_rate = check_vat(c.vat_rate.as_deref().or(rate.as_ref().map(|r| r.vat_rate.as_str())).unwrap_or(&default_vat))?;
        let unit = opt_s(&c.unit).or(rate.as_ref().map(|r| r.unit.clone())).unwrap_or_else(|| "db".into());
        let ledger = opt_s(&c.ledger_code).or(rate.as_ref().map(|r| r.ledger_code.clone())).unwrap_or_default();
        let payer = match (&c.person_id, opt_s(&c.payer_name)) {
            (_, Some(n)) => n,
            (Some(p), None) => person_name(&self.conn, p)?,
            (None, None) => return Err(CoreError::validation("payerName", "payer required").with("code", "required")),
        };
        let net = unit_net * quantity;
        let vat = vat_of(net, &vat_rate);
        let id = new_id();
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO charges (id, cemetery_id, person_id, payer_name, fee_rate_id, description, quantity, unit, unit_net_huf, vat_rate, net_huf, vat_huf, gross_huf,
                                  ledger_code, entity, entity_id, override_reason, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?18,?19)",
            params![
                id,
                c.cemetery_id,
                c.person_id.as_deref().filter(|x| !x.is_empty()),
                payer,
                rate.as_ref().map(|r| r.id.clone()),
                description,
                quantity,
                unit,
                unit_net,
                vat_rate,
                net,
                vat,
                net + vat,
                ledger,
                opt_s(&c.entity),
                opt_s(&c.entity_id),
                opt_s(&c.override_reason),
                ts,
                s.user_name
            ],
        )?;
        let out = self.get_charge_on(&tx, &id)?;
        audit::record(&tx, s, "charge", &id, "create", None::<&()>, Some(&out), c.override_reason.as_deref())?;
        tx.commit()?;
        Ok(out)
    }

    fn get_charge_on(&self, conn: &crate::sql::Connection, id: &str) -> Result<Charge> {
        conn.query_row(&format!("SELECT {CHARGE_COLS} FROM charges WHERE id = ?1"), params![id], map_charge)
            .optional()?
            .ok_or_else(|| CoreError::not_found("charge", id))
    }

    pub fn list_charges(&self, cemetery_id: &str, status: Option<&str>, person_id: Option<&str>) -> Result<Vec<Charge>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {CHARGE_COLS} FROM charges WHERE cemetery_id = ?1 AND (?2 IS NULL OR status = ?2) AND (?3 IS NULL OR person_id = ?3)
             ORDER BY created_at DESC LIMIT 500"
        ))?;
        let rows = st.query_map(params![cemetery_id, status, person_id], map_charge)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn void_charge(&self, s: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        s.require(Permission::RecordPayments)?;
        let reason = require_reason(reason)?;
        let c = self.get_charge_on(&self.conn, id)?;
        if c.status != "open" {
            return Err(CoreError::new("charge_not_open", "only open charges can be voided; storno the invoice instead"));
        }
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE charges SET status='void', voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1",
            params![id, now_ts(), s.user_name, reason],
        )?;
        audit::record_simple(&tx, s, "charge", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    /// FR-PEN-02: a lease with a fee becomes a charge for the holder automatically.
    pub fn charge_for_lease(&self, s: &Session, lease: &Lease, cemetery_id: &str) -> Result<Option<Charge>> {
        if lease.fee_huf <= 0 || lease.fee_exempt {
            return Ok(None);
        }
        let exists: Option<String> = self
            .conn
            .query_row("SELECT id FROM charges WHERE entity = 'lease' AND entity_id = ?1 AND status <> 'void'", params![lease.id], |r| r.get(0))
            .optional()?;
        if exists.is_some() {
            return Ok(None);
        }
        let ident: String = self.conn.query_row("SELECT identifier FROM plots WHERE id = ?1", params![lease.plot_id], |r| r.get(0))?;
        let what = if lease.kind == "renewal" { "Sírhely újraváltása" } else { "Sírhely megváltása" };
        let rate_vat = match &lease.fee_rate_id {
            Some(id) => self.conn.query_row("SELECT vat_rate FROM fee_rates WHERE id = ?1", params![id], |r| r.get::<_, String>(0)).optional()?,
            None => None,
        };
        let payer = match (&lease.person_id, &lease.person_name) {
            (Some(p), _) => person_name(&self.conn, p)?,
            (None, Some(n)) if !n.is_empty() => n.clone(),
            _ => return Ok(None),
        };
        Ok(Some(self.add_charge(
            s,
            &ChargeInput {
                cemetery_id: cemetery_id.to_string(),
                person_id: lease.person_id.clone(),
                payer_name: Some(payer),
                fee_rate_id: lease.fee_rate_id.clone(),
                description: Some(format!("{what}: {ident}, {} év ({}–{})", lease.duration_years, lease.start_date, lease.expiry_date)),
                unit_net_huf: Some(lease.fee_huf),
                vat_rate: rate_vat,
                entity: Some("lease".into()),
                entity_id: Some(lease.id.clone()),
                override_reason: Some("a megváltásnál megadott díj".into()),
                ..Default::default()
            },
        )?))
    }

    // ---------------------------------------------------------------- invoices (FR-PEN-03/04)

    pub fn get_invoice(&self, id: &str) -> Result<Invoice> {
        self.conn
            .query_row(&format!("SELECT {INVOICE_COLS} FROM invoices i WHERE i.id = ?1"), params![id], map_invoice)
            .optional()?
            .ok_or_else(|| CoreError::not_found("invoice", id))
    }

    pub fn list_invoices(&self, cemetery_id: &str, status: Option<&str>, overdue_only: bool) -> Result<Vec<Invoice>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {INVOICE_COLS} FROM invoices i WHERE i.cemetery_id = ?1 AND (?2 IS NULL OR i.status = ?2)
               AND (NOT ?3 OR (i.status IN ('issued','partially_paid') AND i.due_date < ?4))
             ORDER BY i.issue_date DESC, i.number DESC LIMIT 500"
        ))?;
        let rows = st.query_map(params![cemetery_id, status, overdue_only, fmt_date(today())], map_invoice)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn invoice_lines(&self, invoice_id: &str) -> Result<Vec<Value>> {
        let mut st = self.conn.prepare(
            "SELECT description, quantity, unit, unit_net_huf, vat_rate, net_huf, vat_huf, gross_huf, ledger_code FROM invoice_lines WHERE invoice_id = ?1 ORDER BY description",
        )?;
        let rows = st.query_map(params![invoice_id], |r| {
            Ok(json!({ "description": r.get::<_, String>(0)?, "quantity": r.get::<_, i64>(1)?, "unit": r.get::<_, String>(2)?, "unitNetHuf": r.get::<_, i64>(3)?,
                       "vatRate": r.get::<_, String>(4)?, "netHuf": r.get::<_, i64>(5)?, "vatHuf": r.get::<_, i64>(6)?, "grossHuf": r.get::<_, i64>(7)?,
                       "ledgerCode": r.get::<_, String>(8)? }))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Invoices open charges of one payer. With the operator's Számlázz.hu key
    /// the invoice is issued at once; with the ASP it waits for the number
    /// the ASP gives (FR-PEN-04); `proforma` makes a díjbekérő.
    pub fn create_invoice(&self, s: &Session, inp: &InvoiceInput) -> Result<Invoice> {
        s.require(Permission::RecordPayments)?;
        if inp.charge_ids.is_empty() {
            return Err(CoreError::validation("chargeIds", "select at least one item").with("code", "required"));
        }
        for (f, v) in [("buyerName", &inp.buyer_name), ("postcode", &inp.buyer_postcode), ("city", &inp.buyer_city), ("address", &inp.buyer_address)] {
            non_empty(f, v)?;
        }
        if !matches!(inp.payment_method.as_str(), "cash" | "transfer" | "card") {
            return Err(CoreError::validation("paymentMethod", "payment method required").with("code", "required"));
        }
        let provider = settings::get(&self.conn, "finance.provider", None)?.unwrap_or_else(|| "szamlazz".into());
        if provider == "asp" && inp.proforma {
            return Err(CoreError::new("proforma_unavailable", "proformas are issued in the ASP"));
        }
        let mut charges = vec![];
        for id in &inp.charge_ids {
            let c = self.get_charge_on(&self.conn, id)?;
            if c.status != "open" || c.cemetery_id != inp.cemetery_id {
                return Err(CoreError::new("charge_not_open", "an item is already invoiced or voided").with("id", id));
            }
            charges.push(c);
        }
        let issue = today();
        let fulfil = match &inp.fulfilment_date {
            Some(d) if !d.is_empty() => parse_date(d)?,
            _ => issue,
        };
        let due_days = inp.due_days.unwrap_or(settings::get_i64(&self.conn, "finance.payment_due_days", None, 8)?);
        let due = if inp.payment_method == "cash" { issue } else { issue + chrono::Duration::days(due_days.max(0)) };
        let (net, vat): (i64, i64) = charges.iter().fold((0, 0), |(n, v), c| (n + c.net_huf, v + c.vat_huf));
        let address = format!("{} {}, {}", inp.buyer_postcode.trim(), inp.buyer_city.trim(), inp.buyer_address.trim());
        let id = new_id();
        let ts = now_ts();
        let (number, status, pdf, log) = if provider == "asp" {
            (String::new(), "pending_asp".to_string(), None, "ASP-ben kiállítandó".to_string())
        } else {
            let key = settings::get(&self.conn, "internal.szamlazz_key", None)?;
            let seq: i64 = self.conn.query_row("SELECT count(*) + 1 FROM invoices", [], |r| r.get(0))?;
            let doc = Document {
                proforma: inp.proforma,
                issue_date: fmt_date(issue),
                fulfilment_date: fmt_date(fulfil),
                due_date: fmt_date(due),
                payment_method: match inp.payment_method.as_str() {
                    "cash" => "Készpénz".into(),
                    "card" => "Bankkártya".into(),
                    _ => "Átutalás".into(),
                },
                note: settings::get(&self.conn, "finance.invoice_note", None)?.unwrap_or_default(),
                order_number: String::new(),
                proforma_number: None,
                paid: inp.payment_method == "cash" && !inp.proforma,
                buyer: Party {
                    name: inp.buyer_name.trim().into(),
                    tax_number: inp.buyer_tax_number.trim().into(),
                    postcode: inp.buyer_postcode.trim().into(),
                    city: inp.buyer_city.trim().into(),
                    address: inp.buyer_address.trim().into(),
                    email: inp.buyer_email.trim().into(),
                },
                lines: charges
                    .iter()
                    .map(|c| Line {
                        name: c.description.clone(),
                        quantity: c.quantity,
                        unit: c.unit.clone(),
                        unit_net: c.unit_net_huf,
                        vat_percent: c.vat_rate.parse().unwrap_or(0),
                        comment: String::new(),
                    })
                    .collect(),
            };
            let issued = szamlazz::issue_with(key.as_deref(), &doc, seq)?;
            (issued.number, if doc.paid { "paid".to_string() } else { "issued".to_string() }, issued.pdf, issued.log)
        };
        let kind = if inp.proforma { "proforma" } else { "invoice" };
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO invoices (id, cemetery_id, kind, provider, number, person_id, buyer_name, buyer_address, buyer_tax_number, buyer_email,
                                   issue_date, fulfilment_date, due_date, payment_method, net_huf, vat_huf, gross_huf, paid_huf, status, pdf, provider_log,
                                   created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?22,?23)",
            params![
                id,
                inp.cemetery_id,
                kind,
                provider,
                number,
                inp.person_id.as_deref().filter(|x| !x.is_empty()),
                inp.buyer_name.trim(),
                address,
                inp.buyer_tax_number.trim(),
                inp.buyer_email.trim(),
                fmt_date(issue),
                fmt_date(fulfil),
                fmt_date(due),
                inp.payment_method,
                net,
                vat,
                net + vat,
                if status == "paid" { net + vat } else { 0 },
                status,
                pdf,
                log,
                ts,
                s.user_name
            ],
        )?;
        for c in &charges {
            tx.execute(
                "INSERT INTO invoice_lines (id, invoice_id, charge_id, description, quantity, unit, unit_net_huf, vat_rate, net_huf, vat_huf, gross_huf, ledger_code)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![new_id(), id, c.id, c.description, c.quantity, c.unit, c.unit_net_huf, c.vat_rate, c.net_huf, c.vat_huf, c.gross_huf, c.ledger_code],
            )?;
            if !inp.proforma {
                tx.execute(
                    "UPDATE charges SET status='invoiced', invoice_id=?2, updated_at=?3, updated_by=?4 WHERE id=?1",
                    params![c.id, id, ts, s.user_name],
                )?;
            }
        }
        // cash invoices are paid at the desk: payment + receipt
        if status == "paid" {
            self.insert_payment(
                &tx,
                s,
                &PaymentInput {
                    cemetery_id: inp.cemetery_id.clone(),
                    invoice_id: Some(id.clone()),
                    person_id: inp.person_id.clone(),
                    payer_name: Some(inp.buyer_name.clone()),
                    amount_huf: net + vat,
                    method: "cash".into(),
                    paid_on: Some(fmt_date(issue)),
                    reference: number.clone(),
                    notes: String::new(),
                },
                None,
            )?;
        }
        let out = self.get_invoice_on(&tx, &id)?;
        audit::record(&tx, s, "invoice", &id, "create", None::<&()>, Some(&out), None)?;
        tx.commit()?;
        Ok(out)
    }

    fn get_invoice_on(&self, conn: &crate::sql::Connection, id: &str) -> Result<Invoice> {
        conn.query_row(&format!("SELECT {INVOICE_COLS} FROM invoices i WHERE i.id = ?1"), params![id], map_invoice)
            .optional()?
            .ok_or_else(|| CoreError::not_found("invoice", id))
    }

    /// FR-PEN-04: the number the ASP gave the invoice.
    pub fn record_asp_number(&self, s: &Session, id: &str, number: &str) -> Result<Invoice> {
        s.require(Permission::RecordPayments)?;
        non_empty("number", number)?;
        let inv = self.get_invoice(id)?;
        if inv.status != "pending_asp" {
            return Err(CoreError::new("invoice_not_pending", "this invoice already has a number"));
        }
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE invoices SET number=?2, status=CASE WHEN paid_huf >= gross_huf THEN 'paid' WHEN paid_huf > 0 THEN 'partially_paid' ELSE 'issued' END, updated_at=?3, updated_by=?4 WHERE id=?1",
            params![id, number.trim(), now_ts(), s.user_name],
        )?;
        tx.execute("UPDATE charges SET status='invoiced' WHERE invoice_id=?1", params![id])?;
        audit::record_simple(&tx, s, "invoice", id, "asp_number", Some(number.trim()))?;
        tx.commit()?;
        self.get_invoice(id)
    }

    /// Storno: the invoice is reversed by a storno invoice; its items open again.
    pub fn storno_invoice(&self, s: &Session, id: &str, reason: Option<&str>) -> Result<Invoice> {
        s.require(Permission::RecordPayments)?;
        let reason = require_reason(reason)?;
        let inv = self.get_invoice(id)?;
        if inv.kind != "invoice" || inv.status == "storno" {
            return Err(CoreError::new("invoice_not_stornoable", "only issued invoices can be reversed"));
        }
        if inv.paid_huf > 0 {
            return Err(CoreError::new("invoice_paid", "refund the payment first"));
        }
        let (number, log) = if inv.provider == "szamlazz" && inv.status != "pending_asp" {
            let key = settings::get(&self.conn, "internal.szamlazz_key", None)?;
            let st = szamlazz::storno_with(key.as_deref(), &inv.number, &fmt_date(today()))?;
            (st.number, st.log)
        } else {
            (String::new(), "ASP-ben sztornózandó".to_string())
        };
        let sid = new_id();
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO invoices (id, cemetery_id, kind, provider, number, storno_of, person_id, buyer_name, buyer_address, buyer_tax_number, buyer_email,
                                   issue_date, fulfilment_date, due_date, payment_method, net_huf, vat_huf, gross_huf, paid_huf, status, provider_log,
                                   created_at, created_by, updated_at, updated_by)
             SELECT ?2, cemetery_id, 'storno', provider, ?3, id, person_id, buyer_name, buyer_address, buyer_tax_number, buyer_email,
                    ?4, fulfilment_date, ?4, payment_method, -net_huf, -vat_huf, -gross_huf, 0, 'storno', ?5, ?6, ?7, ?6, ?7
             FROM invoices WHERE id = ?1",
            params![id, sid, number, fmt_date(today()), log, ts, s.user_name],
        )?;
        tx.execute("UPDATE invoices SET status='storno', updated_at=?2, updated_by=?3 WHERE id=?1", params![id, ts, s.user_name])?;
        tx.execute("UPDATE charges SET status='open', invoice_id=NULL, updated_at=?2, updated_by=?3 WHERE invoice_id=?1", params![id, ts, s.user_name])?;
        audit::record_simple(&tx, s, "invoice", id, "storno", Some(&reason))?;
        tx.commit()?;
        self.get_invoice(&sid)
    }

    pub fn invoice_pdf(&self, id: &str) -> Result<(String, Vec<u8>)> {
        let row: Option<(String, Option<Vec<u8>>)> =
            self.conn.query_row("SELECT number, pdf FROM invoices WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        match row {
            Some((n, Some(pdf))) => Ok((n, pdf)),
            _ => Err(CoreError::new("pdf_unavailable", "no PDF for this invoice")),
        }
    }

    // ---------------------------------------------------------------- payments (FR-PEN-05)

    pub fn record_payment(&self, s: &Session, p: &PaymentInput) -> Result<Value> {
        s.require(Permission::RecordPayments)?;
        let tx = crate::db::tx(&self.conn)?;
        let id = self.insert_payment(&tx, s, p, None)?;
        tx.commit()?;
        Ok(json!({ "id": id }))
    }

    fn insert_payment(&self, conn: &crate::sql::Connection, s: &Session, p: &PaymentInput, bank_tx_id: Option<&str>) -> Result<String> {
        if p.amount_huf == 0 {
            return Err(CoreError::validation("amountHuf", "amount required").with("code", "required"));
        }
        if !matches!(p.method.as_str(), "cash" | "transfer" | "card") {
            return Err(CoreError::validation("method", "payment method required").with("code", "required"));
        }
        let paid_on = match &p.paid_on {
            Some(d) if !d.is_empty() => fmt_date(parse_date(d)?),
            _ => fmt_date(today()),
        };
        let (payer, person) = match &p.invoice_id {
            Some(iid) => {
                let inv = self.get_invoice_on(conn, iid)?;
                if inv.status == "storno" {
                    return Err(CoreError::new("invoice_not_open", "this invoice was reversed"));
                }
                (p.payer_name.clone().filter(|x| !x.trim().is_empty()).unwrap_or(inv.buyer_name), p.person_id.clone().or(inv.person_id))
            }
            None => (opt_s(&p.payer_name).ok_or_else(|| CoreError::validation("payerName", "payer required").with("code", "required"))?, p.person_id.clone()),
        };
        let id = new_id();
        let ts = now_ts();
        conn.execute(
            "INSERT INTO payments (id, cemetery_id, invoice_id, person_id, payer_name, amount_huf, method, paid_on, reference, bank_tx_id, notes, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?12,?13)",
            params![id, p.cemetery_id, p.invoice_id, person, payer, p.amount_huf, p.method, paid_on, p.reference.trim(), bank_tx_id, p.notes.trim(), ts, s.user_name],
        )?;
        if p.method == "cash" {
            // the desk issues a receipt (or a payout slip for a refund)
            let kind = if p.amount_huf > 0 { "in" } else { "out" };
            let desc = match &p.invoice_id {
                Some(iid) => format!("Befizetés, számla: {}", self.get_invoice_on(conn, iid)?.number),
                None => "Befizetés".into(),
            };
            let entry = self.insert_cash_entry(conn, s, &p.cemetery_id, &paid_on, kind, p.amount_huf.abs(), &payer, &desc, Some(&id))?;
            conn.execute("UPDATE payments SET cash_entry_id = ?2 WHERE id = ?1", params![id, entry.id])?;
        }
        if let Some(iid) = &p.invoice_id {
            refresh_invoice_paid(conn, iid)?;
        }
        audit::record_simple(conn, s, "payment", &id, "create", Some(&format!("{} Ft, {}", p.amount_huf, p.method)))?;
        Ok(id)
    }

    pub fn void_payment(&self, s: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        s.require(Permission::RecordPayments)?;
        let reason = require_reason(reason)?;
        let (invoice, cash): (Option<String>, Option<String>) = self
            .conn
            .query_row("SELECT invoice_id, cash_entry_id FROM payments WHERE id = ?1 AND voided_at IS NULL", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| CoreError::not_found("payment", id))?;
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute("UPDATE payments SET voided_at=?2, voided_by=?3, void_reason=?4 WHERE id=?1", params![id, ts, s.user_name, reason])?;
        if let Some(c) = cash {
            // fails when the cash day is already closed (then book a payout instead)
            tx.execute("UPDATE cash_entries SET voided_at=?2, voided_by=?3, void_reason=?4 WHERE id=?1", params![c, ts, s.user_name, reason])?;
        }
        tx.execute("UPDATE bank_transactions SET status='unmatched', payment_id=NULL WHERE payment_id=?1", params![id])?;
        if let Some(i) = invoice {
            refresh_invoice_paid(&tx, &i)?;
        }
        audit::record_simple(&tx, s, "payment", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_payments(&self, cemetery_id: &str, from: &str, to: &str) -> Result<Vec<Value>> {
        let mut st = self.conn.prepare(
            "SELECT p.id, p.paid_on, p.payer_name, p.amount_huf, p.method, p.reference, i.number, p.voided_at, c.doc_number
             FROM payments p LEFT JOIN invoices i ON i.id = p.invoice_id LEFT JOIN cash_entries c ON c.id = p.cash_entry_id
             WHERE p.cemetery_id = ?1 AND p.paid_on BETWEEN ?2 AND ?3 ORDER BY p.paid_on DESC, p.created_at DESC",
        )?;
        let rows = st.query_map(params![cemetery_id, from, to], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "paidOn": r.get::<_, String>(1)?, "payerName": r.get::<_, String>(2)?, "amountHuf": r.get::<_, i64>(3)?,
                       "method": r.get::<_, String>(4)?, "reference": r.get::<_, String>(5)?, "invoiceNumber": r.get::<_, Option<String>>(6)?,
                       "voidedAt": r.get::<_, Option<String>>(7)?, "receipt": r.get::<_, Option<String>>(8)? }))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------------------------------------------------------------- cash desk (FR-PEN-06)

    #[allow(clippy::too_many_arguments)]
    fn insert_cash_entry(
        &self,
        conn: &crate::sql::Connection,
        s: &Session,
        cemetery_id: &str,
        date: &str,
        kind: &str,
        amount: i64,
        partner: &str,
        description: &str,
        payment_id: Option<&str>,
    ) -> Result<CashEntry> {
        s.require(Permission::CashDesk)?;
        if amount <= 0 {
            return Err(CoreError::validation("amountHuf", "amount must be positive"));
        }
        non_empty("partnerName", partner)?;
        let year = &date[..4];
        // gap-free numbering: the row lock on the sequence serialises concurrent desks
        conn.execute(
            "INSERT INTO cash_sequences (cemetery_id, year, kind, next_value) VALUES (?1, ?2, ?3, 1) ON CONFLICT DO NOTHING",
            params![cemetery_id, year, kind],
        )?;
        let n: i64 = conn.query_row(
            "UPDATE cash_sequences SET next_value = next_value + 1 WHERE cemetery_id=?1 AND year=?2 AND kind=?3 RETURNING next_value - 1",
            params![cemetery_id, year, kind],
            |r| r.get(0),
        )?;
        let number = format!("{}-{year}/{n:04}", if kind == "in" { "BE" } else { "KI" });
        let id = new_id();
        let ts = now_ts();
        conn.execute(
            "INSERT INTO cash_entries (id, cemetery_id, entry_date, kind, doc_number, amount_huf, partner_name, description, payment_id, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?10,?11)",
            params![id, cemetery_id, date, kind, number, amount, partner.trim(), description.trim(), payment_id, ts, s.user_name],
        )?;
        audit::record_simple(conn, s, "cash_entry", &id, "create", Some(&format!("{number}: {amount} Ft")))?;
        Ok(CashEntry {
            id,
            cemetery_id: cemetery_id.into(),
            entry_date: date.into(),
            kind: kind.into(),
            doc_number: number,
            amount_huf: amount,
            partner_name: partner.trim().into(),
            description: description.trim().into(),
            payment_id: payment_id.map(String::from),
            voided_at: None,
        })
    }

    /// A cash movement not tied to an invoice (e.g. petty cash out).
    pub fn add_cash_entry(
        &self,
        s: &Session,
        cemetery_id: &str,
        date: Option<&str>,
        kind: &str,
        amount: i64,
        partner: &str,
        description: &str,
    ) -> Result<CashEntry> {
        if !matches!(kind, "in" | "out") {
            return Err(CoreError::validation("kind", "in or out"));
        }
        let date = date.filter(|d| !d.is_empty()).map(|d| parse_date(d).map(fmt_date)).transpose()?.unwrap_or_else(|| fmt_date(today()));
        let tx = crate::db::tx(&self.conn)?;
        let e = self.insert_cash_entry(&tx, s, cemetery_id, &date, kind, amount, partner, description, None)?;
        tx.commit()?;
        Ok(e)
    }

    fn cash_balance_before(&self, cemetery_id: &str, date: &str) -> Result<i64> {
        // the last closing before the day, plus everything since
        let last: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT closing_date, counted_huf FROM cash_closings WHERE cemetery_id=?1 AND closing_date < ?2 ORDER BY closing_date DESC LIMIT 1",
                params![cemetery_id, date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (since, base) = last.unwrap_or(("0000-00-00".into(), settings::get_i64(&self.conn, "finance.cash_opening_huf", Some(cemetery_id), 0)?));
        let moves: i64 = self.conn.query_row(
            "SELECT coalesce(sum(CASE WHEN kind='in' THEN amount_huf ELSE -amount_huf END), 0) FROM cash_entries
             WHERE cemetery_id=?1 AND voided_at IS NULL AND entry_date > ?2 AND entry_date < ?3",
            params![cemetery_id, since, date],
            |r| r.get(0),
        )?;
        Ok(base + moves)
    }

    /// FR-PEN-06: the cash book of one day with opening and closing balance.
    pub fn cash_day(&self, s: &Session, cemetery_id: &str, date: &str) -> Result<Value> {
        s.require(Permission::CashDesk)?;
        let date = fmt_date(parse_date(date)?);
        let opening = self.cash_balance_before(cemetery_id, &date)?;
        let mut st = self.conn.prepare(
            "SELECT id, cemetery_id, entry_date, kind, doc_number, amount_huf, partner_name, description, payment_id, voided_at
             FROM cash_entries WHERE cemetery_id=?1 AND entry_date=?2 ORDER BY kind, doc_number",
        )?;
        let entries: Vec<CashEntry> = st
            .query_map(params![cemetery_id, date], |r| {
                Ok(CashEntry {
                    id: r.get(0)?,
                    cemetery_id: r.get(1)?,
                    entry_date: r.get(2)?,
                    kind: r.get(3)?,
                    doc_number: r.get(4)?,
                    amount_huf: r.get(5)?,
                    partner_name: r.get(6)?,
                    description: r.get(7)?,
                    payment_id: r.get(8)?,
                    voided_at: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        let sum = |k: &str| entries.iter().filter(|e| e.kind == k && e.voided_at.is_none()).map(|e| e.amount_huf).sum::<i64>();
        let (inn, out) = (sum("in"), sum("out"));
        let closing: Option<Value> = self
            .conn
            .query_row(
                "SELECT counted_huf, difference_note, closed_at, closed_by FROM cash_closings WHERE cemetery_id=?1 AND closing_date=?2",
                params![cemetery_id, date],
                |r| Ok(json!({ "countedHuf": r.get::<_, i64>(0)?, "note": r.get::<_, String>(1)?, "closedAt": r.get::<_, String>(2)?, "closedBy": r.get::<_, Option<String>>(3)? })),
            )
            .optional()?;
        Ok(
            json!({ "date": date, "openingHuf": opening, "inHuf": inn, "outHuf": out, "closingHuf": opening + inn - out, "entries": entries, "closed": closing }),
        )
    }

    /// Closes the day: counted cash recorded, the day can no longer change.
    pub fn close_cash_day(&self, s: &Session, cemetery_id: &str, date: &str, counted: i64, note: &str) -> Result<Value> {
        s.require(Permission::CashDesk)?;
        let day = self.cash_day(s, cemetery_id, date)?;
        let expected = day["closingHuf"].as_i64().unwrap_or(0);
        if counted != expected && note.trim().chars().count() < 3 {
            return Err(CoreError::new("cash_difference", "explain the difference between counted and booked cash").with("difference", counted - expected));
        }
        let date = day["date"].as_str().unwrap_or_default().to_string();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO cash_closings (cemetery_id, closing_date, opening_huf, in_huf, out_huf, closing_huf, counted_huf, difference_note, closed_at, closed_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                cemetery_id,
                date,
                day["openingHuf"].as_i64(),
                day["inHuf"].as_i64(),
                day["outHuf"].as_i64(),
                expected,
                counted,
                note.trim(),
                now_ts(),
                s.user_name
            ],
        )?;
        audit::record_simple(&tx, s, "cash_day", &format!("{cemetery_id}:{date}"), "close", Some(&format!("{counted} Ft")))?;
        tx.commit()?;
        self.cash_day(s, cemetery_id, &date)
    }

    // ---------------------------------------------------------------- bank (FR-PEN-07)

    /// Imports a bank statement (CSV from the internet bank). Columns are found
    /// by their header: date, amount, partner, reference. Lines seen before are skipped.
    pub fn import_bank_csv(&self, s: &Session, cemetery_id: &str, file_name: &str, content_b64: &str) -> Result<Value> {
        s.require(Permission::RecordPayments)?;
        let table = crate::io::read_upload(file_name, content_b64, None)?;
        let find = |names: &[&str]| {
            table.headers.iter().position(|h| {
                let h = crate::text::normalize(h);
                names.iter().any(|n| h.contains(n))
            })
        };
        let c_date = find(&["datum", "konyveles", "date"]).ok_or_else(|| CoreError::new("bank_columns", "date column not found").with("column", "dátum"))?;
        let c_amount =
            find(&["osszeg", "amount", "jovairas"]).ok_or_else(|| CoreError::new("bank_columns", "amount column not found").with("column", "összeg"))?;
        let c_partner = find(&["partner", "nev", "kedvezmenyezett", "befizeto", "ellenoldal"]);
        let c_ref = find(&["kozlemeny", "megjegyzes", "reference", "leiras"]);
        let (mut added, mut skipped) = (0, 0);
        let tx = crate::db::tx(&self.conn)?;
        for row in &table.rows {
            let get = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|v| v.trim().to_string()).unwrap_or_default();
            let date = match crate::io::parse_date_loose(&get(Some(c_date))) {
                Some(d) => fmt_date(d),
                None => {
                    skipped += 1;
                    continue;
                }
            };
            let raw = get(Some(c_amount)).replace([' ', '\u{a0}', '+'], "").replace("Ft", "").replace(',', ".");
            let Ok(amount) = raw.parse::<f64>() else {
                skipped += 1;
                continue;
            };
            let amount = amount.round() as i64;
            let (partner, reference) = (get(c_partner), get(c_ref));
            let fp = sha256_hex(format!("{date}|{amount}|{partner}|{reference}").as_bytes());
            let n = tx.execute(
                "INSERT INTO bank_transactions (id, cemetery_id, booked_on, amount_huf, partner, reference, fingerprint, created_at, created_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT DO NOTHING",
                params![new_id(), cemetery_id, date, amount, partner, reference, fp, now_ts(), s.user_name],
            )?;
            if n == 1 {
                added += 1
            } else {
                skipped += 1
            }
        }
        audit::record_simple(&tx, s, "bank_import", cemetery_id, "import", Some(&format!("{file_name}: {added} új tétel")))?;
        tx.commit()?;
        Ok(json!({ "added": added, "skipped": skipped }))
    }

    /// Bank lines with a suggested invoice for the unmatched ones.
    pub fn list_bank(&self, cemetery_id: &str, status: Option<&str>) -> Result<Vec<BankTx>> {
        let open = self
            .list_invoices(cemetery_id, None, false)?
            .into_iter()
            .filter(|i| matches!(i.status.as_str(), "issued" | "partially_paid") && i.kind == "invoice")
            .collect::<Vec<_>>();
        let mut st = self.conn.prepare(
            "SELECT id, booked_on, amount_huf, partner, reference, status, payment_id FROM bank_transactions
             WHERE cemetery_id=?1 AND (?2 IS NULL OR status=?2) ORDER BY booked_on DESC LIMIT 500",
        )?;
        let rows: Vec<BankTx> = st
            .query_map(params![cemetery_id, status], |r| {
                Ok(BankTx {
                    id: r.get(0)?,
                    booked_on: r.get(1)?,
                    amount_huf: r.get(2)?,
                    partner: r.get(3)?,
                    reference: r.get(4)?,
                    status: r.get(5)?,
                    payment_id: r.get(6)?,
                    suggested_invoice_id: None,
                    suggested_invoice_number: None,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows
            .into_iter()
            .map(|mut b| {
                if b.status == "unmatched" && b.amount_huf > 0 {
                    if let Some(i) = suggest(&b, &open) {
                        b.suggested_invoice_id = Some(i.id.clone());
                        b.suggested_invoice_number = Some(i.number.clone());
                    }
                }
                b
            })
            .collect())
    }

    /// Books a bank line as the payment of an invoice (or ignores it).
    pub fn match_bank(&self, s: &Session, tx_id: &str, invoice_id: Option<&str>) -> Result<()> {
        s.require(Permission::RecordPayments)?;
        let (cemetery, booked, amount, partner, reference, status): (String, String, i64, String, String, String) = self
            .conn
            .query_row("SELECT cemetery_id, booked_on, amount_huf, partner, reference, status FROM bank_transactions WHERE id=?1", params![tx_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .optional()?
            .ok_or_else(|| CoreError::not_found("bank_transaction", tx_id))?;
        if status != "unmatched" {
            return Err(CoreError::new("bank_already_matched", "this line is already handled"));
        }
        let tx = crate::db::tx(&self.conn)?;
        match invoice_id {
            None => {
                tx.execute("UPDATE bank_transactions SET status='ignored' WHERE id=?1", params![tx_id])?;
            }
            Some(iid) => {
                let pid = self.insert_payment(
                    &tx,
                    s,
                    &PaymentInput {
                        cemetery_id: cemetery,
                        invoice_id: Some(iid.to_string()),
                        person_id: None,
                        payer_name: Some(partner).filter(|p| !p.is_empty()),
                        amount_huf: amount,
                        method: "transfer".into(),
                        paid_on: Some(booked),
                        reference,
                        notes: String::new(),
                    },
                    Some(tx_id),
                )?;
                tx.execute("UPDATE bank_transactions SET status='matched', payment_id=?2 WHERE id=?1", params![tx_id, pid])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- receivables (FR-PEN-08)

    pub fn record_reminder(&self, s: &Session, invoice_id: &str, channel: &str) -> Result<Invoice> {
        s.require(Permission::RecordPayments)?;
        if !matches!(channel, "letter" | "email") {
            return Err(CoreError::validation("channel", "letter or email"));
        }
        let level: i64 = self.conn.query_row("SELECT count(*) + 1 FROM reminders WHERE invoice_id=?1", params![invoice_id], |r| r.get(0))?;
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO reminders (id, invoice_id, sent_on, channel, level, created_at, created_by) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![new_id(), invoice_id, fmt_date(today()), channel, level, now_ts(), s.user_name],
        )?;
        audit::record_simple(&tx, s, "invoice", invoice_id, "reminder", Some(&format!("{level}. felszólítás, {channel}")))?;
        tx.commit()?;
        self.get_invoice(invoice_id)
    }

    /// Open amounts per payer (FR-PEN-08).
    pub fn receivables(&self, cemetery_id: &str) -> Result<Vec<Value>> {
        let mut st = self.conn.prepare(
            "SELECT buyer_name, count(*), sum(gross_huf - paid_huf), min(due_date),
                    sum(CASE WHEN due_date < ?2 THEN gross_huf - paid_huf ELSE 0 END)
             FROM invoices WHERE cemetery_id=?1 AND kind='invoice' AND status IN ('issued','partially_paid')
             GROUP BY buyer_name ORDER BY 5 DESC, 3 DESC",
        )?;
        let rows = st.query_map(params![cemetery_id, fmt_date(today())], |r| {
            Ok(json!({ "buyerName": r.get::<_, String>(0)?, "invoices": r.get::<_, i64>(1)?, "openHuf": r.get::<_, i64>(2)?, "oldestDue": r.get::<_, String>(3)?, "overdueHuf": r.get::<_, i64>(4)? }))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------------------------------------------------------------- reports (FR-PEN-09)

    /// Revenue by item (ledger code + description) and the VAT summary for a period.
    pub fn revenue_report(&self, cemetery_id: Option<&str>, from: &str, to: &str) -> Result<Value> {
        let mut st = self.conn.prepare(
            "SELECT coalesce(nullif(l.ledger_code, ''), '–'), l.description, l.vat_rate, sum(l.net_huf), sum(l.vat_huf), sum(l.gross_huf)
             FROM invoice_lines l JOIN invoices i ON i.id = l.invoice_id
             WHERE i.kind = 'invoice' AND i.status NOT IN ('pending_asp') AND i.fulfilment_date BETWEEN ?1 AND ?2 AND (?3 IS NULL OR i.cemetery_id = ?3)
               AND i.status <> 'storno'
             GROUP BY 1, 2, 3 ORDER BY 1, 2",
        )?;
        let lines: Vec<Value> = st
            .query_map(params![from, to, cemetery_id], |r| {
                Ok(json!({ "ledgerCode": r.get::<_, String>(0)?, "description": r.get::<_, String>(1)?, "vatRate": r.get::<_, String>(2)?,
                           "netHuf": r.get::<_, i64>(3)?, "vatHuf": r.get::<_, i64>(4)?, "grossHuf": r.get::<_, i64>(5)? }))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let mut vat: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for l in &lines {
            let e = vat.entry(l["vatRate"].as_str().unwrap_or("").to_string()).or_default();
            e.0 += l["netHuf"].as_i64().unwrap_or(0);
            e.1 += l["vatHuf"].as_i64().unwrap_or(0);
        }
        let paid: i64 = self.conn.query_row(
            "SELECT coalesce(sum(amount_huf), 0) FROM payments WHERE voided_at IS NULL AND paid_on BETWEEN ?1 AND ?2 AND (?3 IS NULL OR cemetery_id = ?3)",
            params![from, to, cemetery_id],
            |r| r.get(0),
        )?;
        let total = |k: &str| lines.iter().map(|l| l[k].as_i64().unwrap_or(0)).sum::<i64>();
        Ok(json!({
            "from": from, "to": to, "lines": lines,
            "vatSummary": vat.into_iter().map(|(k, (n, v))| json!({ "vatRate": k, "netHuf": n, "vatHuf": v })).collect::<Vec<_>>(),
            "totals": { "netHuf": total("netHuf"), "vatHuf": total("vatHuf"), "grossHuf": total("grossHuf"), "paidHuf": paid }
        }))
    }

    /// Accounting export rows (invoice lines with ledger codes).
    pub fn ledger_rows(&self, cemetery_id: Option<&str>, from: &str, to: &str) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
        let cols = [
            "Számlaszám",
            "Kelt",
            "Teljesítés",
            "Vevő",
            "Adószám",
            "Tétel",
            "Főkönyvi szám",
            "ÁFA",
            "Nettó",
            "ÁFA összeg",
            "Bruttó",
            "Fizetési mód",
            "Állapot",
        ];
        let mut st = self.conn.prepare(
            "SELECT i.number, i.issue_date, i.fulfilment_date, i.buyer_name, i.buyer_tax_number, l.description, l.ledger_code, l.vat_rate,
                    CASE WHEN i.kind='storno' THEN -l.net_huf ELSE l.net_huf END, CASE WHEN i.kind='storno' THEN -l.vat_huf ELSE l.vat_huf END,
                    CASE WHEN i.kind='storno' THEN -l.gross_huf ELSE l.gross_huf END, i.payment_method, i.status
             FROM invoices i JOIN invoice_lines l ON l.invoice_id = coalesce(i.storno_of, i.id)
             WHERE i.kind IN ('invoice','storno') AND i.issue_date BETWEEN ?1 AND ?2 AND (?3 IS NULL OR i.cemetery_id = ?3)
             ORDER BY i.issue_date, i.number",
        )?;
        let rows = st.query_map(params![from, to, cemetery_id], |r| {
            (0..13)
                .map(|i| {
                    let v: SqlValue = r.get(i)?;
                    Ok(match v {
                        SqlValue::Integer(n) => json!(n),
                        SqlValue::Text(t) => json!(t),
                        SqlValue::Real(f) => json!(f),
                        _ => Value::Null,
                    })
                })
                .collect::<crate::sql::Result<Vec<Value>>>()
        })?;
        Ok((cols.iter().map(|c| c.to_string()).collect(), rows.collect::<std::result::Result<_, _>>()?))
    }

    /// ASP export: pending invoices as rows to key into the ASP (FR-PEN-04).
    pub fn asp_rows(&self, cemetery_id: &str) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
        let cols = [
            "Belső azonosító",
            "Vevő",
            "Cím",
            "Adószám",
            "Tétel",
            "Mennyiség",
            "Egység",
            "Nettó egységár",
            "ÁFA",
            "Nettó",
            "Bruttó",
            "Főkönyvi szám",
            "Teljesítés",
            "Fizetési határidő",
        ];
        let mut st = self.conn.prepare(
            "SELECT substr(i.id, 1, 8), i.buyer_name, i.buyer_address, i.buyer_tax_number, l.description, l.quantity, l.unit, l.unit_net_huf, l.vat_rate,
                    l.net_huf, l.gross_huf, l.ledger_code, i.fulfilment_date, i.due_date
             FROM invoices i JOIN invoice_lines l ON l.invoice_id = i.id WHERE i.cemetery_id=?1 AND i.status='pending_asp' ORDER BY i.created_at",
        )?;
        let rows = st.query_map(params![cemetery_id], |r| {
            (0..14)
                .map(|i| {
                    let v: SqlValue = r.get(i)?;
                    Ok(match v {
                        SqlValue::Integer(n) => json!(n),
                        SqlValue::Text(t) => json!(t),
                        _ => Value::Null,
                    })
                })
                .collect::<crate::sql::Result<Vec<Value>>>()
        })?;
        Ok((cols.iter().map(|c| c.to_string()).collect(), rows.collect::<std::result::Result<_, _>>()?))
    }

    /// The operator's own Számlázz.hu agent key (admin only, never exported or shown again).
    pub fn set_szamlazz_key(&self, s: &Session, key: &str) -> Result<()> {
        s.require(Permission::ManageSettings)?;
        if s.role != crate::auth::Role::Admin {
            return Err(CoreError::forbidden("admin"));
        }
        settings::set_internal(&self.conn, "internal.szamlazz_key", key.trim())?;
        audit::record_simple(&self.conn, s, "setting", "internal.szamlazz_key", "update", Some(if key.trim().is_empty() { "törölve" } else { "beállítva" }))?;
        Ok(())
    }

    pub fn finance_status(&self) -> Result<Value> {
        let key = settings::get(&self.conn, "internal.szamlazz_key", None)?.filter(|k| !k.is_empty()).is_some();
        Ok(json!({
            "provider": settings::get(&self.conn, "finance.provider", None)?.unwrap_or_else(|| "szamlazz".into()),
            "agentKeySet": key,
            "dueDays": settings::get_i64(&self.conn, "finance.payment_due_days", None, 8)?,
            "defaultVat": settings::get(&self.conn, "finance.default_vat", None)?.unwrap_or_else(|| "27".into()),
            "vatCodes": VAT_CODES,
        }))
    }
}

fn refresh_invoice_paid(conn: &crate::sql::Connection, invoice_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE invoices SET paid_huf = (SELECT coalesce(sum(amount_huf), 0) FROM payments WHERE invoice_id = ?1 AND voided_at IS NULL),
                status = CASE WHEN status IN ('storno', 'pending_asp') THEN status
                              WHEN (SELECT coalesce(sum(amount_huf), 0) FROM payments WHERE invoice_id = ?1 AND voided_at IS NULL) >= gross_huf THEN 'paid'
                              WHEN (SELECT coalesce(sum(amount_huf), 0) FROM payments WHERE invoice_id = ?1 AND voided_at IS NULL) > 0 THEN 'partially_paid'
                              ELSE 'issued' END
         WHERE id = ?1",
        params![invoice_id],
    )?;
    Ok(())
}

/// Invoice number in the reference wins; otherwise same open amount and a name match.
fn suggest<'a>(b: &BankTx, open: &'a [Invoice]) -> Option<&'a Invoice> {
    let reference = b.reference.to_uppercase();
    if let Some(i) = open.iter().find(|i| !i.number.is_empty() && reference.contains(&i.number.to_uppercase())) {
        return Some(i);
    }
    let partner = crate::text::normalize(&b.partner);
    let candidates: Vec<&Invoice> = open.iter().filter(|i| i.gross_huf - i.paid_huf == b.amount_huf).collect();
    candidates
        .iter()
        .find(|i| {
            let name = crate::text::normalize(&i.buyer_name);
            !partner.is_empty() && (partner.contains(&name) || name.split(' ').filter(|w| w.len() > 2).all(|w| partner.contains(w)))
        })
        .copied()
        .or(if candidates.len() == 1 { candidates.first().copied() } else { None })
}

/// Dynamic parameter lists for reports.
#[allow(dead_code)]
fn _params(v: Vec<SqlValue>) -> crate::sql::ParamsFromIter {
    params_from_iter(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Store, Session, String, String) {
        let store = Store::in_memory().unwrap();
        let s = Session::system();
        crate::sample::load_sample(&store, &s).unwrap();
        let cem: String = store.conn.query_row("SELECT id FROM cemeteries LIMIT 1", [], |r| r.get(0)).unwrap();
        let rate = store
            .add_fee_rate(
                &s,
                &FeeRate {
                    cemetery_id: cem.clone(),
                    fee_kind: "chapel".into(),
                    name: "Ravatalozó használata".into(),
                    amount_huf: 20_000,
                    vat_rate: "27".into(),
                    ledger_code: "9113".into(),
                    valid_from: "2020-01-01".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        (store, s, cem, rate.id)
    }

    fn invoice_input(cem: &str, ids: Vec<String>, method: &str) -> InvoiceInput {
        InvoiceInput {
            cemetery_id: cem.into(),
            charge_ids: ids,
            buyer_name: "Kovács János".into(),
            buyer_postcode: "9999".into(),
            buyer_city: "Mintavölgy".into(),
            buyer_address: "Fő utca 1.".into(),
            payment_method: method.into(),
            ..Default::default()
        }
    }

    #[test]
    fn charge_invoice_payment_cycle() {
        let (store, s, cem, rate) = setup();
        let c = store
            .add_charge(
                &s,
                &ChargeInput {
                    cemetery_id: cem.clone(),
                    payer_name: Some("Kovács János".into()),
                    fee_rate_id: Some(rate.clone()),
                    quantity: Some(2),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!((c.net_huf, c.vat_huf, c.gross_huf), (40_000, 10_800, 50_800));
        // a changed price needs a reason
        let err = store
            .add_charge(
                &s,
                &ChargeInput {
                    cemetery_id: cem.clone(),
                    payer_name: Some("X".into()),
                    fee_rate_id: Some(rate.clone()),
                    unit_net_huf: Some(1),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "reason_required");
        let inv = store.create_invoice(&s, &invoice_input(&cem, vec![c.id.clone()], "transfer")).unwrap();
        assert!(inv.number.starts_with("TESZT-S-"), "{}", inv.number);
        assert_eq!((inv.status.as_str(), inv.gross_huf), ("issued", 50_800));
        // the item cannot be invoiced twice
        assert_eq!(store.create_invoice(&s, &invoice_input(&cem, vec![c.id.clone()], "transfer")).unwrap_err().code, "charge_not_open");
        store
            .record_payment(
                &s,
                &PaymentInput {
                    cemetery_id: cem.clone(),
                    invoice_id: Some(inv.id.clone()),
                    amount_huf: 20_000,
                    method: "transfer".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(store.get_invoice(&inv.id).unwrap().status, "partially_paid");
        store
            .record_payment(
                &s,
                &PaymentInput { cemetery_id: cem.clone(), invoice_id: Some(inv.id.clone()), amount_huf: 30_800, method: "cash".into(), ..Default::default() },
            )
            .unwrap();
        let paid = store.get_invoice(&inv.id).unwrap();
        assert_eq!((paid.status.as_str(), paid.paid_huf), ("paid", 50_800));
        let rep = store.revenue_report(Some(&cem), "2000-01-01", "2100-01-01").unwrap();
        assert_eq!(rep["totals"]["grossHuf"], 50_800);
        assert_eq!(rep["lines"][0]["ledgerCode"], "9113");
        assert!(crate::audit::verify_chain(&store.conn).unwrap().ok);
    }

    #[test]
    fn storno_reopens_items_and_asp_flow() {
        let (store, s, cem, rate) = setup();
        let c = store
            .add_charge(
                &s,
                &ChargeInput { cemetery_id: cem.clone(), payer_name: Some("Nagy Éva".into()), fee_rate_id: Some(rate.clone()), ..Default::default() },
            )
            .unwrap();
        let inv = store.create_invoice(&s, &invoice_input(&cem, vec![c.id.clone()], "transfer")).unwrap();
        assert_eq!(store.storno_invoice(&s, &inv.id, None).unwrap_err().code, "reason_required");
        let st = store.storno_invoice(&s, &inv.id, Some("hibás vevőnév")).unwrap();
        assert_eq!((st.kind.as_str(), st.gross_huf), ("storno", -25_400));
        assert_eq!(store.list_charges(&cem, Some("open"), None).unwrap().len(), 1);
        // ASP: waits for the number, then counts as issued
        settings::set_many(&store.conn, &s, None, &[("finance.provider".to_string(), "asp".to_string())].into_iter().collect()).unwrap();
        let inv = store.create_invoice(&s, &invoice_input(&cem, vec![c.id.clone()], "transfer")).unwrap();
        assert_eq!((inv.status.as_str(), inv.number.as_str()), ("pending_asp", ""));
        assert_eq!(store.asp_rows(&cem).unwrap().1.len(), 1);
        let inv = store.record_asp_number(&s, &inv.id, "ASP-2026/15").unwrap();
        assert_eq!((inv.status.as_str(), inv.number.as_str()), ("issued", "ASP-2026/15"));
    }

    #[test]
    fn cash_desk_numbers_and_closing() {
        let (store, s, cem, _) = setup();
        let d = fmt_date(today());
        let a = store.add_cash_entry(&s, &cem, Some(&d), "in", 10_000, "Kiss Margit", "Sírhely-gondozás").unwrap();
        let b = store.add_cash_entry(&s, &cem, Some(&d), "in", 5_000, "Tóth Pál", "Ravatalozó").unwrap();
        let o = store.add_cash_entry(&s, &cem, Some(&d), "out", 2_000, "Posta", "Bélyeg").unwrap();
        let y = &d[..4];
        assert_eq!(a.doc_number, format!("BE-{y}/0001"));
        assert_eq!(b.doc_number, format!("BE-{y}/0002"));
        assert_eq!(o.doc_number, format!("KI-{y}/0001"));
        let day = store.cash_day(&s, &cem, &d).unwrap();
        assert_eq!(day["closingHuf"], 13_000);
        // a difference needs an explanation
        assert_eq!(store.close_cash_day(&s, &cem, &d, 12_000, "").unwrap_err().code, "cash_difference");
        store.close_cash_day(&s, &cem, &d, 13_000, "").unwrap();
        // the closed day cannot change
        assert_eq!(store.add_cash_entry(&s, &cem, Some(&d), "in", 1, "X", "Y").unwrap_err().code, "cash_day_closed");
    }

    #[test]
    fn bank_import_and_matching() {
        let (store, s, cem, rate) = setup();
        let c = store
            .add_charge(&s, &ChargeInput { cemetery_id: cem.clone(), payer_name: Some("Szabó László".into()), fee_rate_id: Some(rate), ..Default::default() })
            .unwrap();
        let inv = store.create_invoice(&s, &invoice_input(&cem, vec![c.id], "transfer")).unwrap();
        let csv = format!("Könyvelés dátuma;Összeg;Partner neve;Közlemény\n2026.11.05.;25 400;SZABÓ LÁSZLÓ;{}\n2026.11.05.;1 000;Valaki;semmi\n", inv.number);
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(csv.as_bytes());
        assert_eq!(store.import_bank_csv(&s, &cem, "kivonat.csv", &b64).unwrap()["added"], 2);
        // the same statement again adds nothing
        assert_eq!(store.import_bank_csv(&s, &cem, "kivonat.csv", &b64).unwrap()["added"], 0);
        let lines = store.list_bank(&cem, Some("unmatched")).unwrap();
        let hit = lines.iter().find(|b| b.amount_huf == 25_400).unwrap();
        assert_eq!(hit.suggested_invoice_id.as_deref(), Some(inv.id.as_str()));
        store.match_bank(&s, &hit.id, Some(&inv.id)).unwrap();
        assert_eq!(store.get_invoice(&inv.id).unwrap().status, "paid");
        assert_eq!(store.receivables(&cem).unwrap().len(), 0);
    }
}
