//! Import (FR-IMP-01/03), full export (FR-IMP-04) and the migration check
//! report (UC-05 step 4).

use std::collections::BTreeMap;
use std::path::Path;

use crate::sql::params;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit;
use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::model::*;
use crate::store::Store;
use crate::text::normalize;
use crate::util::fmt_date;

// ---------------------------------------------------------------------------
// Reading tables
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub sheet_names: Vec<String>,
}

/// Reads an uploaded file given as base64 (browser) by writing it to a temp file.
pub fn read_upload(file_name: &str, content_base64: &str, sheet: Option<&str>) -> Result<Table> {
    let bytes = crate::pdf::base64_decode_pub(content_base64).ok_or_else(|| CoreError::new("import_format", "invalid upload"))?;
    let ext = Path::new(file_name).extension().and_then(|e| e.to_str()).unwrap_or("csv").to_lowercase();
    if ext == "csv" || ext == "txt" || ext == "tsv" {
        return read_csv(&bytes);
    }
    let tmp = std::env::temp_dir().join(format!("lunda-upload-{}.{ext}", crate::util::new_id()));
    std::fs::write(&tmp, &bytes)?;
    let t = read_table(&tmp, sheet);
    let _ = std::fs::remove_file(&tmp);
    t
}

/// Reads .xlsx/.xls/.ods (first or named sheet) or .csv/.txt (UTF-8 or Windows-1250, ; , or tab).
pub fn read_table(path: &Path, sheet: Option<&str>) -> Result<Table> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "csv" | "txt" | "tsv" => read_csv(&std::fs::read(path)?),
        "xlsx" | "xlsm" | "xls" | "ods" => read_sheet(path, sheet),
        _ => Err(CoreError::new("import_format", "unsupported file type").with("ext", ext)),
    }
}

fn decode(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1250.decode(b).0.into_owned(),
    }
}

pub fn read_csv(bytes: &[u8]) -> Result<Table> {
    let text = decode(bytes);
    let first = text.lines().next().unwrap_or("");
    let delim = b";\t,".iter().copied().max_by_key(|d| first.matches(*d as char).count()).unwrap_or(b';');
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).flexible(true).has_headers(true).from_reader(text.as_bytes());
    let headers = rdr.headers().map_err(|e| CoreError::new("import_format", e.to_string()))?.iter().map(|h| h.trim().to_string()).collect();
    let mut rows = vec![];
    for r in rdr.records() {
        let r = r.map_err(|e| CoreError::new("import_format", e.to_string()))?;
        let row: Vec<String> = r.iter().map(|c| c.trim().to_string()).collect();
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    Ok(Table { headers, rows, sheet_names: vec![] })
}

fn read_sheet(path: &Path, sheet: Option<&str>) -> Result<Table> {
    use calamine::{open_workbook_auto, Data, DataType, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| CoreError::new("import_format", e.to_string()))?;
    let names = wb.sheet_names().to_vec();
    let range = match sheet {
        Some(n) => wb.worksheet_range(n).map_err(|e| CoreError::new("import_format", e.to_string()))?,
        None => wb
            .worksheet_range_at(0)
            .ok_or_else(|| CoreError::new("import_format", "empty workbook"))?
            .map_err(|e| CoreError::new("import_format", e.to_string()))?,
    };
    let cell = |c: &Data| -> String {
        match c {
            Data::Empty => String::new(),
            Data::String(s) => s.trim().to_string(),
            Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
            Data::Float(f) => f.to_string(),
            Data::Int(i) => i.to_string(),
            Data::Bool(b) => b.to_string(),
            Data::DateTime(_) | Data::DateTimeIso(_) => c.as_date().map(fmt_date).unwrap_or_default(),
            other => other.to_string(),
        }
    };
    let mut it = range.rows();
    let headers = it.next().map(|r| r.iter().map(cell).collect()).unwrap_or_default();
    let rows = it.map(|r| r.iter().map(cell).collect::<Vec<_>>()).filter(|r: &Vec<String>| r.iter().any(|c| !c.is_empty())).collect();
    Ok(Table { headers, rows, sheet_names: names })
}

// ---------------------------------------------------------------------------
// Field mapping
// ---------------------------------------------------------------------------

/// Importable target fields (keys used by the UI mapping wizard).
pub const TARGETS: &[&str] = &[
    "folio",
    "eventDate",
    "eventType",
    "fullName",
    "familyName",
    "givenName",
    "namePrefix",
    "birthName",
    "sex",
    "birthPlace",
    "birthDate",
    "motherName",
    "lastResidence",
    "deathPlace",
    "deathDate",
    "deathCertNo",
    "plotIdentifier",
    "section",
    "row",
    "plot",
    "plotType",
    "holderName",
    "holderAddress",
    "holderPhone",
    "leaseStart",
    "leaseYears",
    "leaseExpiry",
    "leaseFee",
    "notes",
];

/// Guesses the mapping from Hungarian/English header names.
pub fn guess_mapping(headers: &[String]) -> BTreeMap<String, usize> {
    let rules: &[(&str, &[&str])] = &[
        ("folio", &["folyoszam", "folyo szam", "sorszam", "folio"]),
        ("eventDate", &["temetes napja", "temetes datuma", "temetes ideje", "temetes", "elhelyezes napja"]),
        ("eventType", &["esemeny", "temetes modja", "temetes jellege"]),
        ("familyName", &["csaladi nev", "vezeteknev", "csaladnev"]),
        ("givenName", &["utonev", "keresztnev"]),
        ("fullName", &["nev", "elhunyt neve", "elhalt neve", "elhunyt"]),
        ("namePrefix", &["elotag"]),
        ("birthName", &["szuletesi nev", "leanykori nev"]),
        ("sex", &["nem"]),
        ("birthPlace", &["szuletesi hely"]),
        ("birthDate", &["szuletesi ido", "szuletesi datum", "szuletett", "szul ido"]),
        ("motherName", &["anyja neve", "anya neve"]),
        ("lastResidence", &["lakohely", "utolso lakohely", "legutolso lakohely", "lakcim"]),
        ("deathPlace", &["elhalalozas helye", "halalozas helye"]),
        ("deathDate", &["elhalalozas ideje", "halalozas ideje", "halalozas datuma", "elhunyt datuma", "meghalt"]),
        ("deathCertNo", &["halottvizsgalati", "bizonyitvany"]),
        ("plotIdentifier", &["sirhely", "hely azonosito", "sirhely azonosito", "azonosito", "temetesi hely"]),
        ("section", &["parcella", "sirhelytabla", "tabla"]),
        ("row", &["sor"]),
        ("plot", &["hely", "sirszam", "hely szama"]),
        ("plotType", &["helytipus", "sirhely tipusa", "tipus"]),
        ("holderName", &["rendelkezesi jogosult", "jogosult", "eltemetteto", "megvalto"]),
        ("holderAddress", &["jogosult lakcime", "jogosult cime", "eltemetteto lakcime", "megvalto cime"]),
        ("holderPhone", &["telefon"]),
        ("leaseStart", &["megvaltas kezdete", "megvaltas datuma", "megvaltva"]),
        ("leaseYears", &["megvaltas ideje", "idotartam", "evek"]),
        ("leaseExpiry", &["lejarat", "ervenyes"]),
        ("leaseFee", &["dij", "osszeg", "megvaltasi dij"]),
        ("notes", &["megjegyzes", "megj"]),
    ];
    let norm: Vec<String> = headers.iter().map(|h| normalize(h).replace(['.', ':'], "")).collect();
    let mut out = BTreeMap::new();
    let mut used = std::collections::HashSet::new();
    // exact matches first, then "contains"
    for pass in 0..2 {
        for (target, keys) in rules {
            if out.contains_key(*target) {
                continue;
            }
            for (i, h) in norm.iter().enumerate() {
                if used.contains(&i) {
                    continue;
                }
                let hit = keys.iter().any(|k| if pass == 0 { h == k } else { h.contains(k) && k.len() > 3 });
                if hit {
                    out.insert(target.to_string(), i);
                    used.insert(i);
                    break;
                }
            }
        }
    }
    out
}

/// Parses Hungarian date notations; year-only values map to January 1.
/// Date only (bank statements and other imports).
pub fn parse_date_loose(s: &str) -> Option<NaiveDate> {
    parse_hu_date(s).map(|(d, _)| d)
}

pub fn parse_hu_date(s: &str) -> Option<(NaiveDate, bool)> {
    let t = s.trim().trim_end_matches('.');
    if t.is_empty() {
        return None;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    let parts: Vec<&str> = t.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).collect();
    if parts.len() == 3 {
        let (a, b, c) = (parts[0], parts[1], parts[2]);
        let (y, m, d) = if a.len() == 4 {
            (a, b, c)
        } else if c.len() == 4 {
            (c, b, a)
        } else {
            return None;
        };
        return NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?).map(|d| (d, false));
    }
    if digits.len() == 8 && parts.len() == 1 {
        return NaiveDate::parse_from_str(&digits, "%Y%m%d").ok().map(|d| (d, false));
    }
    if parts.len() == 1 && digits.len() == 4 {
        let y: i32 = digits.parse().ok()?;
        if (1700..=2200).contains(&y) {
            return NaiveDate::from_ymd_opt(y, 1, 1).map(|d| (d, true));
        }
    }
    // Excel serial number as text
    if parts.len() == 1 && (4..=5).contains(&digits.len()) {
        let n: i64 = digits.parse().ok()?;
        return NaiveDate::from_ymd_opt(1899, 12, 30).and_then(|b| b.checked_add_signed(chrono::Duration::days(n))).map(|d| (d, false));
    }
    None
}

const PREFIXES: &[&str] = &["dr.", "dr", "özv.", "özv", "id.", "ifj.", "prof.", "vitéz"];

/// "dr. Kovács János" → ("dr.", "Kovács", "János") – Hungarian order.
pub fn split_name(full: &str) -> (String, String, String) {
    let mut words: Vec<&str> = full.split_whitespace().collect();
    let mut prefix = vec![];
    while let Some(w) = words.first() {
        if PREFIXES.contains(&w.to_lowercase().as_str()) {
            prefix.push(*w);
            words.remove(0);
        } else {
            break;
        }
    }
    let family = words.first().map(|s| s.to_string()).unwrap_or_default();
    let given = if words.len() > 1 { words[1..].join(" ") } else { String::new() };
    (prefix.join(" "), family, given)
}

fn parse_event_type(s: &str, default: &str) -> String {
    let n = normalize(s);
    if n.is_empty() {
        return default.into();
    }
    for (k, keys) in [
        ("urn_release", &["kiadas", "kiadva"][..]),
        ("urn_removal", &["kiemel"][..]),
        ("ash_scattering", &["szoras", "szorva"][..]),
        ("common_grave", &["kozos"][..]),
        ("reburial", &["ratemet"][..]),
        ("urn_placement", &["urna"][..]),
        ("coffin_burial", &["koporso", "temetes"][..]),
    ] {
        if keys.iter().any(|x| n.contains(x)) {
            return k.into();
        }
    }
    if crate::store::ALL_EVENT_TYPES.contains(&s) {
        return s.into();
    }
    default.into()
}

fn parse_identifier(s: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = s.split(['/', '-', ' ', '.']).map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.len() == 3 {
        Some((parts[0].to_uppercase(), parts[1].into(), parts[2].into()))
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportOptions {
    pub cemetery_id: String,
    pub mapping: BTreeMap<String, usize>,
    pub default_event_type: String,
    pub default_plot_type_id: String,
    /// keep the paper folio numbers (FR-IMP-03)
    pub keep_folio: bool,
    pub dry_run: bool,
    pub skip_invalid: bool,
    pub create_leases: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RowError {
    /// 1-based data row number (header excluded)
    pub row: usize,
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub dry_run: bool,
    pub committed: bool,
    pub rows_total: usize,
    pub rows_ok: usize,
    pub rows_failed: usize,
    pub plots_created: usize,
    pub warnings: Vec<RowError>,
    pub errors: Vec<RowError>,
    /// normalised first rows for the preview
    pub preview: Vec<BTreeMap<String, String>>,
}

fn get(row: &[String], m: &BTreeMap<String, usize>, k: &str) -> String {
    m.get(k).and_then(|i| row.get(*i)).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Imports rows as register events with deceased, plot, holder and lease.
/// The whole run happens inside a savepoint: a dry run (próbafuttatás)
/// executes every rule and then rolls everything back.
pub fn import_rows(store: &Store, s: &Session, table: &Table, o: &ImportOptions) -> Result<ImportReport> {
    s.require(Permission::Import)?;
    store.get_cemetery(&o.cemetery_id)?;
    let default_type = if o.default_event_type.is_empty() { "coffin_burial".to_string() } else { o.default_event_type.clone() };
    let default_plot_type = if o.default_plot_type_id.is_empty() { store.get_plot_type("single")?.id } else { o.default_plot_type_id.clone() };
    let types = store.list_plot_types()?;
    let m = &o.mapping;
    let mut rep = ImportReport { dry_run: o.dry_run, rows_total: table.rows.len(), ..Default::default() };
    let plots_before: i64 = store.conn.query_row("SELECT count(*) FROM plots WHERE cemetery_id=?1", [&o.cemetery_id], |r| r.get(0))?;
    let outer = crate::db::tx(&store.conn)?;
    for (i, row) in table.rows.iter().enumerate() {
        let n = i + 1;
        let err = |field: &str, code: &str, msg: String| RowError { row: n, field: field.into(), code: code.into(), message: msg };
        // ---- parse
        let mut d = Deceased::default();
        let full = get(row, m, "fullName");
        if !full.is_empty() && get(row, m, "familyName").is_empty() {
            let (p, f, g) = split_name(&full);
            d.name_prefix = p;
            d.family_name = f;
            d.given_name = g;
        } else {
            d.family_name = get(row, m, "familyName");
            d.given_name = get(row, m, "givenName");
            d.name_prefix = get(row, m, "namePrefix");
        }
        d.birth_name = get(row, m, "birthName");
        d.sex = match normalize(&get(row, m, "sex")).as_str() {
            "ferfi" | "f" | "male" | "m" => "male".into(),
            "no" | "n" | "female" => "female".into(),
            _ => String::new(),
        };
        d.birth_place = get(row, m, "birthPlace");
        d.mother_name = get(row, m, "motherName");
        d.last_residence = get(row, m, "lastResidence");
        d.death_place = get(row, m, "deathPlace");
        d.death_cert_no = get(row, m, "deathCertNo");
        let mut notes = get(row, m, "notes");
        let mut row_errors = vec![];
        let mut date_field = |field: &str, notes: &mut String| -> Option<String> {
            let raw = get(row, m, field);
            if raw.is_empty() {
                return None;
            }
            match parse_hu_date(&raw) {
                Some((dt, year_only)) => {
                    if year_only {
                        rep.warnings.push(err(field, "year_only", format!("csak évszám: {raw}")));
                        if !notes.is_empty() {
                            notes.push_str("; ");
                        }
                        notes.push_str(&format!("{field}: csak évszám ismert"));
                    }
                    Some(fmt_date(dt))
                }
                None => {
                    row_errors.push(err(field, "invalid_date", format!("érvénytelen dátum: {raw}")));
                    None
                }
            }
        };
        d.birth_date = date_field("birthDate", &mut notes);
        d.death_date = date_field("deathDate", &mut notes);
        let event_date = date_field("eventDate", &mut notes).or_else(|| d.death_date.clone());
        let lease_start = date_field("leaseStart", &mut notes);
        let lease_expiry = date_field("leaseExpiry", &mut notes);
        d.notes = notes.clone();
        let event_type = parse_event_type(&get(row, m, "eventType"), &default_type);
        let folio = get(row, m, "folio");
        let folio: Option<i64> = if o.keep_folio && !folio.is_empty() {
            match folio.trim_end_matches('.').parse() {
                Ok(f) => Some(f),
                Err(_) => {
                    row_errors.push(err("folio", "invalid_number", format!("érvénytelen folyószám: {folio}")));
                    None
                }
            }
        } else {
            None
        };
        let ident = get(row, m, "plotIdentifier");
        let (sec, rw, pl) = if !ident.is_empty() {
            match parse_identifier(&ident) {
                Some(t) => t,
                None => {
                    row_errors.push(err("plotIdentifier", "invalid_identifier", format!("érvénytelen helyazonosító: {ident}")));
                    (String::new(), String::new(), String::new())
                }
            }
        } else {
            (get(row, m, "section").to_uppercase(), get(row, m, "row"), get(row, m, "plot"))
        };
        let ptype = {
            let raw = normalize(&get(row, m, "plotType"));
            if raw.is_empty() {
                default_plot_type.clone()
            } else {
                types
                    .iter()
                    .find(|t| normalize(&t.name) == raw || normalize(&t.code) == raw || normalize(&t.name).contains(&raw))
                    .map(|t| t.id.clone())
                    .unwrap_or(default_plot_type.clone())
            }
        };
        if event_date.is_none() {
            row_errors.push(err("eventDate", "required", "hiányzik a temetés napja (vagy a halálozás ideje)".into()));
        }
        if sec.is_empty() || rw.is_empty() || pl.is_empty() {
            row_errors.push(err("plotIdentifier", "required", "hiányzik a temetési hely (parcella/sor/hely)".into()));
        }
        if i < 20 {
            let mut p = BTreeMap::new();
            p.insert("name".into(), crate::text::full_name(&d.name_prefix, &d.family_name, &d.given_name));
            p.insert("birthDate".into(), d.birth_date.clone().unwrap_or_default());
            p.insert("deathDate".into(), d.death_date.clone().unwrap_or_default());
            p.insert("eventDate".into(), event_date.clone().unwrap_or_default());
            p.insert("eventType".into(), event_type.clone());
            p.insert("plot".into(), format!("{sec}/{rw}/{pl}"));
            p.insert("folio".into(), folio.map(|f| f.to_string()).unwrap_or_default());
            p.insert("holder".into(), get(row, m, "holderName"));
            rep.preview.push(p);
        }
        if !row_errors.is_empty() {
            rep.rows_failed += 1;
            rep.errors.extend(row_errors);
            continue;
        }
        // ---- write (each row in its own savepoint)
        let row_tx = crate::db::tx(&store.conn)?;
        let result = (|| -> Result<()> {
            if !d.unknown_identity && d.mother_name.is_empty() {
                // old records often lack the mother's name: keep the entry, flag it
                d.mother_name = "ismeretlen".into();
                rep.warnings.push(err("motherName", "missing", "anyja neve hiányzik – „ismeretlen” került be".into()));
            }
            if d.last_residence.is_empty() {
                d.last_residence = "ismeretlen".into();
                rep.warnings.push(err("lastResidence", "missing", "lakóhely hiányzik – „ismeretlen” került be".into()));
            }
            if d.birth_date.is_none() {
                rep.warnings.push(err("birthDate", "missing", "születési idő hiányzik".into()));
            }
            let plot_id = store.ensure_plot(s, &o.cemetery_id, &sec, &rw, &pl, &ptype)?;
            let holder_name = get(row, m, "holderName");
            let holder = if holder_name.is_empty() {
                None
            } else {
                let (p, f, g) = split_name(&holder_name);
                Some(PersonRef::New {
                    person: Person {
                        name_prefix: p,
                        family_name: f,
                        given_name: g,
                        address: get(row, m, "holderAddress"),
                        phone: get(row, m, "holderPhone"),
                        ..Default::default()
                    },
                })
            };
            let input = BurialInput {
                event_type: event_type.clone(),
                event_date: event_date.clone().unwrap_or_default(),
                container: if event_type == "urn_placement" { "urn".into() } else { "coffin".into() },
                deceased: DeceasedRef::New { deceased: d.clone() },
                plot_id: Some(plot_id.clone()),
                crypt_slot_id: None,
                arranger: holder.clone(),
                holder,
                holder_relation: None,
                lease: LeaseChoice::default(),
                historical_folio: folio,
                override_reason: None,
                notes: notes.clone(),
            };
            if d.birth_date.is_none() {
                // allow import without birth date: the register keeps the gap visible
                let mut dd = d.clone();
                dd.birth_date = None;
                let tx = crate::db::tx(&store.conn)?;
                let id = store.insert_deceased_unchecked(&tx, s, &dd)?;
                tx.commit()?;
                let mut inp = input;
                inp.deceased = DeceasedRef::Existing { id };
                store.record_burial(s, &inp)?;
            } else {
                store.record_burial(s, &input)?;
            }
            // lease from the file (FR-IMP-01) – start/expiry/years/fee
            if o.create_leases {
                if let Some(start) = lease_start.clone().or(event_date.clone()) {
                    let years: Option<i32> = get(row, m, "leaseYears").parse().ok();
                    let fee: Option<i64> = get(row, m, "leaseFee").replace([' ', '.'], "").trim_end_matches("Ft").trim().parse().ok();
                    let has_lease: bool = store.conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL)",
                        [&plot_id],
                        |r| r.get(0),
                    )?;
                    if !has_lease && (years.is_some() || lease_expiry.is_some() || lease_start.is_some()) {
                        let ptype_full = store.get_plot_type(&ptype)?;
                        let duration = years.unwrap_or(ptype_full.default_lease_years.max(1));
                        let expiry =
                            lease_expiry.clone().unwrap_or_else(|| fmt_date(crate::rules::lease_expiry(crate::util::parse_date(&start).unwrap(), duration)));
                        let holder_id = store.primary_holder_id_pub(&plot_id)?;
                        let tx = crate::db::tx(&store.conn)?;
                        store.insert_lease_pub(
                            &tx,
                            s,
                            &plot_id,
                            holder_id.as_deref(),
                            "purchase",
                            &start,
                            duration,
                            &expiry,
                            fee.unwrap_or(0),
                            None,
                            false,
                            None,
                            "importált",
                        )?;
                        tx.commit()?;
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                row_tx.commit()?;
                rep.rows_ok += 1;
            }
            Err(e) => {
                drop(row_tx); // rolls back this row
                rep.rows_failed += 1;
                rep.errors.push(err(e.params.get("field").map(String::as_str).unwrap_or(""), &e.code, e.message.clone()));
            }
        }
    }
    let plots_after: i64 = store.conn.query_row("SELECT count(*) FROM plots WHERE cemetery_id=?1", [&o.cemetery_id], |r| r.get(0))?;
    rep.plots_created = (plots_after - plots_before).max(0) as usize;
    let commit = !o.dry_run && (rep.rows_failed == 0 || o.skip_invalid) && rep.rows_ok > 0;
    if commit {
        audit::record_simple(&outer, s, "import", &o.cemetery_id, "import", Some(&format!("{} sor, {} hiba", rep.rows_ok, rep.rows_failed)))?;
        outer.commit()?;
        crate::db::analyze(&store.conn)?;
        rep.committed = true;
    } // else: dropping `outer` rolls everything back (dry run)
    Ok(rep)
}

// ---------------------------------------------------------------------------
// Export (FR-IMP-04, FR-RIP-05)
// ---------------------------------------------------------------------------

/// Tables in a full export (attachments omitted – binary).
const EXPORT_TABLES: &[&str] = &[
    "cemeteries",
    "plot_types",
    "sections",
    "plot_rows",
    "plots",
    "crypt_slots",
    "plot_restrictions",
    "deceased",
    "persons",
    "interment_events",
    "plot_holders",
    "fee_rates",
    "leases",
    "notices",
    "monuments",
    "users",
    "settings",
    "audit_log",
    "print_log",
    "backup_log",
];

fn table_rows(store: &Store, table: &str) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
    let mut st = store.conn.prepare(&format!("SELECT * FROM {table}"))?;
    let cols: Vec<String> = st.column_names()?;
    let skip: Vec<bool> = cols.iter().map(|c| c == "password_hash" || c == "search_text" || c == "sort_key").collect();
    let mut rows = vec![];
    let mut q = st.query([])?;
    while let Some(r) = q.next()? {
        let mut row = vec![];
        for (i, _) in cols.iter().enumerate() {
            if skip[i] {
                continue;
            }
            let v: crate::sql::Value = r.get(i)?;
            row.push(match v {
                crate::sql::Value::Null => Value::Null,
                crate::sql::Value::Integer(n) => json!(n),
                crate::sql::Value::Real(f) => json!(f),
                crate::sql::Value::Text(t) => json!(t),
                crate::sql::Value::Blob(b) => json!(format!("<{} bájt>", b.len())),
            });
        }
        if table == "settings" && row.first().and_then(|v| v.as_str()).map(|k| k.starts_with("internal.")).unwrap_or(false) {
            continue; // never export the recovery key
        }
        rows.push(row);
    }
    let cols = cols.into_iter().zip(skip).filter(|(_, s)| !s).map(|(c, _)| c).collect();
    Ok((cols, rows))
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn write_xlsx(sheets: &[(String, Vec<String>, Vec<Vec<Value>>)], path: &Path) -> Result<()> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    for (name, cols, rows) in sheets {
        let ws = wb.add_worksheet();
        ws.set_name(name.chars().take(31).collect::<String>()).map_err(|e| CoreError::new("export", e.to_string()))?;
        for (c, h) in cols.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold).map_err(|e| CoreError::new("export", e.to_string()))?;
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let res = match v {
                    Value::Number(n) if n.as_f64().is_some() => ws.write_number((r + 1) as u32, c as u16, n.as_f64().unwrap()).map(|_| ()),
                    Value::Null => Ok(()),
                    other => ws.write_string((r + 1) as u32, c as u16, cell_text(other)).map(|_| ()),
                };
                res.map_err(|e| CoreError::new("export", e.to_string()))?;
            }
        }
        ws.autofit();
    }
    wb.save(path).map_err(|e| CoreError::new("export", e.to_string()))?;
    Ok(())
}

fn write_csv(cols: &[String], rows: &[Vec<Value>], path: &Path) -> Result<()> {
    let mut w = csv::WriterBuilder::new().delimiter(b';').from_writer(Vec::new());
    w.write_record(cols).map_err(|e| CoreError::new("export", e.to_string()))?;
    for r in rows {
        w.write_record(r.iter().map(cell_text)).map_err(|e| CoreError::new("export", e.to_string()))?;
    }
    let mut bytes = b"\xEF\xBB\xBF".to_vec(); // BOM: Excel opens UTF-8 correctly
    bytes.extend(w.into_inner().map_err(|e| CoreError::new("export", e.to_string()))?);
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Exports and returns the generated file.
/// kind: full | register | plots | expiries | report_utilization | report_yearly | migration_check
pub fn export(store: &Store, s: &Session, kind: &str, format: &str, args: &Value, out_dir: &Path) -> Result<crate::pdf::GeneratedFile> {
    s.require(Permission::Export)?;
    let personal = matches!(kind, "full" | "register" | "expiries");
    if personal && !s.can(Permission::ViewPersonal) {
        return Err(CoreError::forbidden("view_personal"));
    }
    std::fs::create_dir_all(out_dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let cem = args.get("cemeteryId").and_then(|v| v.as_str()).map(String::from);
    let sheets: Vec<(String, Vec<String>, Vec<Vec<Value>>)> = match kind {
        "full" => EXPORT_TABLES.iter().map(|t| table_rows(store, t).map(|(c, r)| (t.to_string(), c, r))).collect::<Result<_>>()?,
        "register" => {
            let ev = store.list_events(&EventFilter { cemetery_id: cem.clone(), include_voided: true, ..Default::default() })?.items;
            let cols = [
                "folyószám",
                "temetés napja",
                "esemény",
                "elhunyt neve",
                "születési név",
                "születési idő",
                "anyja neve",
                "legutolsó lakóhely",
                "elhalálozás ideje",
                "hely",
                "eltemettető",
                "eltemettető lakcíme",
                "érvénytelenítve",
                "érvénytelenítés oka",
            ];
            let rows = ev
                .iter()
                .map(|e| {
                    vec![
                        json!(e.folio),
                        json!(e.event_date),
                        json!(e.event_type),
                        json!(e.deceased_name),
                        json!(e.birth_name),
                        json!(e.birth_date),
                        json!(e.mother_name),
                        json!(e.last_residence),
                        json!(e.death_date),
                        json!(e.plot_identifier),
                        json!(e.arranger_name),
                        json!(e.arranger_address),
                        json!(e.voided_at),
                        json!(e.void_reason),
                    ]
                })
                .collect();
            vec![("Nyilvántartó könyv".into(), cols.iter().map(|s| s.to_string()).collect(), rows)]
        }
        "plots" => {
            let p = store.plot_summaries(&PlotFilter { cemetery_id: cem.clone(), ..Default::default() })?.items;
            let cols = ["hely", "típus", "állapot", "védett", "nyugvók száma", "nyugvók", "lejárat", "jogosult"];
            let rows = p
                .iter()
                .map(|p| {
                    vec![
                        json!(p.identifier),
                        json!(p.type_name),
                        json!(p.status.as_str()),
                        json!(p.protected),
                        json!(p.occupants),
                        json!(p.occupant_names),
                        json!(p.lease_expiry),
                        json!(p.holder_name),
                    ]
                })
                .collect();
            vec![("Temetési helyek".into(), cols.iter().map(|s| s.to_string()).collect(), rows)]
        }
        "expiries" => {
            let f: ExpiryFilter =
                serde_json::from_value(args.get("filter").cloned().unwrap_or(json!({"withinDays": 365, "includeExpired": true}))).unwrap_or_default();
            let e = store.expiries(s, &f)?;
            let cols = ["hely", "lejárat", "hátralévő nap", "állapot", "jogosult", "jogosult címe", "nyugvók", "utolsó értesítés"];
            let rows = e
                .iter()
                .map(|r| {
                    vec![
                        json!(r.identifier),
                        json!(r.lease_expiry),
                        json!(r.days_left),
                        json!(r.status.as_str()),
                        json!(r.holder_name),
                        json!(r.holder_address),
                        json!(r.occupant_names),
                        json!(r.last_notice),
                    ]
                })
                .collect();
            vec![("Lejáratok".into(), cols.iter().map(|s| s.to_string()).collect(), rows)]
        }
        "report_utilization" => {
            let u = store.utilization(cem.as_deref())?;
            let mut cols: Vec<String> = vec!["temető".into(), "parcella".into(), "összesen".into()];
            cols.extend(crate::rules::PlotStatus::ALL.iter().map(|s| s.as_str().to_string()));
            let rows = u
                .iter()
                .map(|r| {
                    let mut v = vec![json!(r.cemetery_name), json!(r.section_code), json!(r.total)];
                    v.extend(crate::rules::PlotStatus::ALL.iter().map(|s| json!(r.by_status.get(s.as_str()).copied().unwrap_or(0))));
                    v
                })
                .collect();
            vec![("Kihasználtság".into(), cols, rows)]
        }
        "ledger" => {
            s.require(Permission::RecordPayments)?;
            let from = args.get("from").and_then(|v| v.as_str()).unwrap_or("1900-01-01").to_string();
            let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31").to_string();
            let (cols, rows) = store.ledger_rows(cem.as_deref(), &from, &to)?;
            vec![("Könyvelés".into(), cols, rows)]
        }
        "asp" => {
            s.require(Permission::RecordPayments)?;
            let c = cem.clone().ok_or_else(|| CoreError::new("bad_request", "cemeteryId"))?;
            let (cols, rows) = store.asp_rows(&c)?;
            vec![("ASP tételek".into(), cols, rows)]
        }
        "annual" => {
            let year = args.get("year").and_then(|v| v.as_i64()).unwrap_or(chrono::Datelike::year(&crate::util::today()) as i64 - 1) as i32;
            let r = store.annual_report(s, cem.as_deref(), year)?;
            let rows = |key: &str, fields: &[&str]| -> Vec<Vec<Value>> {
                r[key].as_array().cloned().unwrap_or_default().iter().map(|x| fields.iter().map(|f| x[*f].clone()).collect()).collect()
            };
            let cols = |c: &[&str]| c.iter().map(|x| x.to_string()).collect::<Vec<String>>();
            let summary = vec![
                vec![Value::from("Év"), Value::from(year)],
                vec![Value::from("Temetések összesen"), r["burialsTotal"].clone()],
                vec![Value::from("Szertartások (naptár)"), r["ceremonies"].clone()],
                vec![Value::from("Számlázott bevétel (bruttó Ft)"), r["revenueGrossHuf"].clone()],
                vec![Value::from("Ebből sztornó (bruttó Ft)"), r["reversedHuf"].clone()],
                vec![Value::from("Befizetések (Ft)"), r["paymentsHuf"].clone()],
                vec![Value::from("Kintlévőség (Ft)"), r["receivables"]["openHuf"].clone()],
                vec![Value::from("Ebből lejárt (Ft)"), r["receivables"]["overdueHuf"].clone()],
                vec![Value::from("Házipénztár bevétel (Ft)"), r["cash"]["inHuf"].clone()],
                vec![Value::from("Házipénztár kiadás (Ft)"), r["cash"]["outHuf"].clone()],
                vec![Value::from("Temetési helyek összesen"), r["plots"]["total"].clone()],
            ]
            .into_iter()
            .chain(r["plots"]["byStatus"].as_object().cloned().unwrap_or_default().into_iter().map(|(k, v)| vec![Value::from(format!("Hely állapota: {k}")), v]))
            .collect();
            vec![
                ("Összesítő".into(), cols(&["tétel", "érték"]), summary),
                ("Temetések".into(), cols(&["esemény", "darab"]), rows("burials", &["eventType", "n"])),
                ("Megváltások".into(), cols(&["fajta", "darab", "díj (Ft)"]), rows("leases", &["kind", "n", "feeHuf"])),
                ("Bevétel jogcímenként".into(), cols(&["díjtétel", "főkönyvi szám", "nettó", "ÁFA", "bruttó"]), rows("revenue", &["feeKind", "ledgerCode", "netHuf", "vatHuf", "grossHuf"])),
                ("Elvégzett munkák".into(), cols(&["munka", "darab"]), rows("workDone", &["kind", "n"])),
                ("Ügyiratok".into(), cols(&["ügytípus", "darab", "jóváhagyva", "elutasítva", "folyamatban"]), rows("cases", &["kind", "n", "approved", "rejected", "pending"])),
                (
                    "Kapacitás".into(),
                    cols(&["temető", "parcella", "összes hely", "szabad", "újraértékesíthető", "lejárt", "új temetés/év", "várható betelés (év)"]),
                    rows("capacity", &["cemeteryName", "sectionCode", "total", "free", "resellable", "expired", "perYear", "yearsLeft"]),
                ),
            ]
        }
        "report_yearly" => {
            let y = store.yearly_stats(cem.as_deref())?;
            let mut cols: Vec<String> = vec!["év".into()];
            cols.extend(crate::store::ALL_EVENT_TYPES.iter().map(|s| s.to_string()));
            cols.push("összesen".into());
            let rows = y
                .iter()
                .map(|r| {
                    let mut v = vec![json!(r.year)];
                    v.extend(crate::store::ALL_EVENT_TYPES.iter().map(|t| json!(r.by_type.get(*t).copied().unwrap_or(0))));
                    v.push(json!(r.total));
                    v
                })
                .collect();
            vec![("Éves statisztika".into(), cols, rows)]
        }
        "migration_check" => {
            let c = migration_check(store, cem.as_deref())?;
            let mk = |name: &str, items: &[String]| (name.to_string(), vec!["tétel".to_string()], items.iter().map(|i| vec![json!(i)]).collect::<Vec<_>>());
            vec![
                mk("Elhunytak hely nélkül", &c.deceased_without_plot),
                mk("Megváltott helyek nyugvó nélkül", &c.leased_plots_without_deceased),
                mk("Foglalt helyek megváltás nélkül", &c.occupied_plots_without_lease),
            ]
        }
        _ => return Err(CoreError::new("bad_request", format!("unknown export {kind}"))),
    };
    let base = format!("lunda-{kind}-{stamp}");
    let path = match format {
        "json" => {
            let obj: serde_json::Map<String, Value> = sheets
                .iter()
                .map(|(n, cols, rows)| {
                    let arr: Vec<Value> = rows.iter().map(|r| Value::Object(cols.iter().cloned().zip(r.iter().cloned()).collect())).collect();
                    (n.clone(), Value::Array(arr))
                })
                .collect();
            let p = out_dir.join(format!("{base}.json"));
            std::fs::write(
                &p,
                serde_json::to_vec_pretty(&json!({"app": "TEMISZ", "version": env!("CARGO_PKG_VERSION"), "exportedAt": crate::util::now_ts(), "data": obj}))?,
            )?;
            p
        }
        "csv" => {
            // one CSV per table in a folder
            let d = out_dir.join(&base);
            std::fs::create_dir_all(&d)?;
            for (n, cols, rows) in &sheets {
                write_csv(cols, rows, &d.join(format!("{}.csv", crate::text::normalize(n).replace(' ', "-"))))?;
            }
            d
        }
        _ => {
            let p = out_dir.join(format!("{base}.xlsx"));
            write_xlsx(&sheets, &p)?;
            p
        }
    };
    let sha = if path.is_file() { crate::util::sha256_hex(&std::fs::read(&path)?) } else { String::new() };
    // NFR-BIZ-03: exports containing personal data are logged
    audit::record_simple(&store.conn, s, "export", kind, if personal { "export_personal" } else { "export" }, Some(&format!("{format} {sha}")))?;
    Ok(crate::pdf::GeneratedFile {
        path: path.display().to_string(),
        file_name: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
        pages: sheets.len(),
        sha256: sha,
        content_hash: None,
        volume_id: None,
    })
}

// ---------------------------------------------------------------------------
// UC-05 step 4: migration check report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCheck {
    pub deceased_without_plot: Vec<String>,
    pub leased_plots_without_deceased: Vec<String>,
    pub occupied_plots_without_lease: Vec<String>,
}

pub fn migration_check(store: &Store, cemetery_id: Option<&str>) -> Result<MigrationCheck> {
    let q = |sql: &str| -> Result<Vec<String>> {
        let mut st = store.conn.prepare(sql)?;
        let rows = st.query_map(params![cemetery_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    };
    Ok(MigrationCheck {
        deceased_without_plot: q("SELECT trim(d.family_name||' '||d.given_name)||' ('||coalesce(substr(d.death_date,1,4),'?')||')' FROM deceased d
             WHERE d.voided_at IS NULL AND (?1 IS NULL OR ?1 IS NOT NULL)
               AND NOT EXISTS (SELECT 1 FROM interment_events e WHERE e.deceased_id=d.id AND e.voided_at IS NULL
                    AND (e.plot_id IS NOT NULL OR e.destination<>'' OR e.receiver_name<>''))
             ORDER BY d.sort_key LIMIT 500")?,
        leased_plots_without_deceased: q(
            "SELECT p.identifier FROM plots p JOIN leases l ON l.plot_id=p.id AND l.status='active' AND l.voided_at IS NULL AND l.kind<>'prepurchase'
             WHERE p.voided_at IS NULL AND (?1 IS NULL OR p.cemetery_id=?1)
               AND NOT EXISTS (SELECT 1 FROM interment_events e WHERE e.plot_id=p.id AND e.voided_at IS NULL)
             ORDER BY p.identifier LIMIT 500",
        )?,
        occupied_plots_without_lease: q("SELECT DISTINCT p.identifier FROM plots p JOIN interment_events e ON e.plot_id=p.id AND e.voided_at IS NULL
             JOIN plot_types t ON t.id=p.plot_type_id
             WHERE p.voided_at IS NULL AND (?1 IS NULL OR p.cemetery_id=?1) AND t.category NOT IN ('scatter','common')
               AND NOT (p.protected=1 AND p.protection_kind='national')
               AND NOT EXISTS (SELECT 1 FROM leases l WHERE l.plot_id=p.id AND l.status='active' AND l.voided_at IS NULL)
             ORDER BY p.identifier LIMIT 500")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hungarian_dates() {
        let d = |s: &str| parse_hu_date(s).map(|(d, y)| (fmt_date(d), y));
        assert_eq!(d("1962.02.18"), Some(("1962-02-18".into(), false)));
        assert_eq!(d("1962. 02. 18."), Some(("1962-02-18".into(), false)));
        assert_eq!(d("1962-2-8"), Some(("1962-02-08".into(), false)));
        assert_eq!(d("18.02.1962"), Some(("1962-02-18".into(), false)));
        assert_eq!(d("19620218"), Some(("1962-02-18".into(), false)));
        assert_eq!(d("1962"), Some(("1962-01-01".into(), true)));
        assert_eq!(d("22695"), Some(("1962-02-18".into(), false))); // Excel serial
        assert_eq!(d("hétfő"), None);
        assert_eq!(d("1962.13.40"), None);
    }

    #[test]
    fn names_in_hungarian_order() {
        assert_eq!(split_name("dr. Kovács János Péter"), ("dr.".into(), "Kovács".into(), "János Péter".into()));
        assert_eq!(split_name("özv. Nagy Istvánné"), ("özv.".into(), "Nagy".into(), "Istvánné".into()));
        assert_eq!(split_name("Kiss"), ("".into(), "Kiss".into(), "".into()));
    }

    #[test]
    fn csv_windows_1250_semicolon() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1250.encode("Név;Születési idő;Hely\nŐri Ödön;1930.05.06;A/1/2\n");
        let t = read_csv(&bytes).unwrap();
        assert_eq!(t.headers, vec!["Név", "Születési idő", "Hely"]);
        assert_eq!(t.rows[0][0], "Őri Ödön");
        let m = guess_mapping(&t.headers);
        assert_eq!(m.get("fullName"), Some(&0));
        assert_eq!(m.get("birthDate"), Some(&1));
        assert_eq!(m.get("plot").or(m.get("plotIdentifier")), Some(&2));
    }

    #[test]
    fn event_type_words() {
        assert_eq!(parse_event_type("Urnás temetés", "coffin_burial"), "urn_placement");
        assert_eq!(parse_event_type("rátemetés", "coffin_burial"), "reburial");
        assert_eq!(parse_event_type("", "coffin_burial"), "coffin_burial");
        assert_eq!(parse_event_type("urna kiadása", "x"), "urn_release");
    }
}
