//! Concrete documents. Each generator lays out pages, adds footers and returns
//! the PDF bytes; [`generate`] writes the file and logs authenticated prints.

use std::path::{Path, PathBuf};

use crate::sql::{params, OptionalExtension};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::engine::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::model::{Cemetery, EventFilter, Lease, Person, Plot};
use crate::rules::PlotStatus;
use crate::settings;
use crate::store::Store;
use crate::util::{new_id, now_ts, sha256_hex, today};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: String,
    pub file_name: String,
    pub pages: usize,
    pub sha256: String,
    /// content digest printed on authenticated books
    pub content_hash: Option<String>,
    pub volume_id: Option<String>,
}

fn hu_date(iso: &str) -> String {
    if iso.len() >= 10 {
        format!("{}. {}. {}.", &iso[0..4], &iso[5..7], &iso[8..10])
    } else if iso.is_empty() {
        "–".into()
    } else {
        iso.into()
    }
}

fn od(v: &Option<String>) -> String {
    v.as_deref().map(hu_date).unwrap_or_else(|| "–".into())
}

fn huf(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(ch);
    }
    format!("{}{} Ft", if n < 0 { "-" } else { "" }, out)
}

const EVENT_HU: &[(&str, &str)] = &[
    ("coffin_burial", "koporsós temetés"),
    ("urn_placement", "urnaelhelyezés"),
    ("reburial", "rátemetés"),
    ("urn_release", "urnakiadás"),
    ("urn_removal", "urnakiemelés"),
    ("ash_scattering", "hamuszórás"),
    ("relocation", "áthelyezés"),
    ("common_grave", "közös sírba helyezés"),
];

fn event_hu(t: &str) -> &str {
    EVENT_HU.iter().find(|(k, _)| *k == t).map(|(_, v)| *v).unwrap_or(t)
}

struct Operator {
    name: String,
    address: String,
    phone: String,
    email: String,
    signer: String,
    signer_title: String,
    regulation: String,
    logo: Option<Vec<u8>>,
}

fn operator(store: &Store) -> Result<Operator> {
    let g = |k: &str| settings::get_or(&store.conn, k, "");
    let logo = g("operator.logo")?;
    let logo = logo.split_once("base64,").and_then(|(_, b)| base64_decode(b));
    Ok(Operator {
        name: g("operator.name")?,
        address: g("operator.address")?,
        phone: g("operator.phone")?,
        email: g("operator.email")?,
        signer: g("signer.name")?,
        signer_title: g("signer.title")?,
        regulation: g("regulation.name")?,
        logo,
    })
}

pub fn base64_decode_pub(s: &str) -> Option<Vec<u8>> {
    base64_decode(s)
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for b in s.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = T.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Letterhead for single documents.
fn letterhead(l: &mut Layout, op: &Operator, title: &str) {
    let m = l.margin;
    let mut x = m;
    if let Some(png) = &op.logo {
        let ly = l.y;
        l.page().ops.push(Op::Image { x: m, y: ly, w: 16.0 * MM, h: 16.0 * MM, png: png.clone() });
        x += 20.0 * MM;
    }
    let y0 = l.y;
    l.text(x, 12.0, FontKind::SansBold, &op.name, BLACK);
    l.y += 15.0;
    let contact = [op.address.as_str(), op.phone.as_str(), op.email.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
    l.text(x, 9.0, FontKind::Sans, &contact, GREY);
    l.y = y0 + 20.0 * MM;
    l.hline(LIGHT, 0.6);
    l.gap(12.0);
    l.text(m, 18.0, FontKind::SerifBold, title, BLACK);
    l.gap(32.0);
}

fn place_date(op: &Operator) -> String {
    let town = op.address.split(',').next().unwrap_or("").trim().trim_start_matches(|c: char| c.is_ascii_digit()).trim().to_string();
    let d = today();
    let prefix = if town.is_empty() { String::new() } else { format!("{town}, ") };
    format!("{prefix}{}. {:02}. {:02}.", d.year(), d.month(), d.day())
}

fn s_arg(args: &Value, k: &str) -> Result<String> {
    args.get(k).and_then(|v| v.as_str()).map(String::from).ok_or_else(|| CoreError::new("bad_request", format!("argument `{k}`")))
}

fn opt_arg(args: &Value, k: &str) -> Option<String> {
    args.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from)
}

fn ids_arg(args: &Value, k: &str) -> Vec<String> {
    args.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Generates a document, writes it into `out_dir` and returns its description.
pub fn generate(store: &Store, s: &Session, doc: &str, args: &Value, out_dir: &Path) -> Result<GeneratedFile> {
    s.require(Permission::Print)?;
    let op = operator(store)?;
    let (pages, title, authenticated): (Vec<Page>, String, Option<(String, String)>) = match doc {
        "register" | "crypt_book" | "inspection" => {
            let cemetery_id = s_arg(args, "cemeteryId")?;
            let from = opt_arg(args, "dateFrom");
            let to = opt_arg(args, "dateTo");
            let volume = new_id()[..8].to_uppercase();
            let (pages, title, hash) = register_book(store, &op, &cemetery_id, from.as_deref(), to.as_deref(), doc, &volume)?;
            (pages, title, Some((volume, hash)))
        }
        "index" => index_doc(store, &op, &s_arg(args, "cemeteryId")?)?,
        "information" => information(store, &op, &s_arg(args, "deceasedId")?)?,
        "lease_certificate" => lease_certificate(store, &op, &s_arg(args, "leaseId")?)?,
        "fee_request" => fee_request(store, &op, &s_arg(args, "leaseId")?)?,
        "expiry_notice" => expiry_notice(store, s, &op, &ids_arg(args, "plotIds"))?,
        "grave_labels" => grave_labels(store, &op, &ids_arg(args, "plotIds"))?,
        "notice_board" => notice_board(store, &op, &ids_arg(args, "plotIds"))?,
        "urn_receipt" => urn_receipt(store, &op, &s_arg(args, "eventId")?)?,
        "monument_permit" => monument_permit(store, &op, &s_arg(args, "monumentId")?)?,
        "case_document" => case_document(store, &op, &s_arg(args, "caseId")?)?,
        "burial_pack" => {
            let symbol =
                opt_arg(args, "symbol").unwrap_or_else(|| settings::get_or(&store.conn, "print.mourning_symbol", "none").unwrap_or_else(|_| "none".into()));
            burial_pack(store, &op, &s_arg(args, "eventId")?, &ids_arg(args, "items"), &symbol)?
        }
        "map" => map_doc(store, &op, &s_arg(args, "cemeteryId")?, Paper::parse(&opt_arg(args, "paper").unwrap_or_else(|| "A3".into())))?,
        "fee_list" => fee_list_doc(store, &op, opt_arg(args, "cemeteryId").as_deref())?,
        "report_utilization" | "report_yearly" => report_doc(store, &op, doc, opt_arg(args, "cemeteryId").as_deref())?,
        _ => return Err(CoreError::new("bad_request", format!("unknown document {doc}"))),
    };
    let bytes = render(&pages, &title)?;
    std::fs::create_dir_all(out_dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("{}-{stamp}.pdf", slug(&title));
    let path: PathBuf = out_dir.join(&file_name);
    std::fs::write(&path, &bytes)?;
    let sha = sha256_hex(&bytes);
    let (volume_id, content_hash) = match authenticated {
        Some((v, h)) => {
            // JOG-04/05: every authenticated print is logged (append-only)
            store.conn.execute(
                "INSERT INTO print_log (id, ts, user_name, document, parameters, page_count, content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![v, now_ts(), s.user_name, doc, args.to_string(), pages.len() as i64, h],
            )?;
            audit::record_simple(&store.conn, s, "print", &v, doc, Some(&h))?;
            (Some(v), Some(h))
        }
        None => {
            audit::record_simple(&store.conn, s, "print", doc, "print", None)?;
            (None, None)
        }
    };
    if doc == "case_document" {
        // FR-ENG-04: the issued document is kept unchanged with the case
        let case_id = s_arg(args, "caseId")?;
        let ts = now_ts();
        store.conn.execute(
            "INSERT INTO attachments (id, entity, entity_id, file_name, mime_type, sha256, content, created_at, created_by, updated_at, updated_by)
             VALUES (?1, 'case', ?2, ?3, 'application/pdf', ?4, ?5, ?6, ?7, ?6, ?7)",
            params![new_id(), case_id, file_name, sha, bytes, ts, s.user_name],
        )?;
        audit::record_simple(&store.conn, s, "case", &case_id, "issue", Some(&title))?;
    }
    Ok(GeneratedFile { path: path.display().to_string(), file_name, pages: pages.len(), sha256: sha, content_hash, volume_id })
}

fn slug(s: &str) -> String {
    crate::text::normalize(s).replace(' ', "-").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect()
}

fn cemetery(store: &Store, id: &str) -> Result<Cemetery> {
    store.get_cemetery(id)
}

// ---------------------------------------------------------------------------
// Authenticated register book and crypt book (FR-NYO-01, JOG-03/04, FR-SIR-03)
// ---------------------------------------------------------------------------

fn monument_text(store: &Store, plot_id: &Option<String>) -> Result<String> {
    let Some(pid) = plot_id else { return Ok(String::new()) };
    let m = store.monuments_for_plot(pid)?;
    Ok(m.iter().filter(|m| m.voided_at.is_none()).map(|m| format!("{}: {}", hu_date(&m.entry_date), m.description)).collect::<Vec<_>>().join("; "))
}

fn holder_text(store: &Store, plot_id: &Option<String>) -> Result<String> {
    let Some(pid) = plot_id else { return Ok(String::new()) };
    let hs = store.holders_for_plot(pid)?;
    Ok(hs
        .iter()
        .filter(|h| h.role == "primary" && h.end_date.is_none() && h.voided_at.is_none())
        .map(|h| format!("{}, {}", h.person_name, h.person_address))
        .collect::<Vec<_>>()
        .join("; "))
}

#[allow(clippy::type_complexity)]
fn register_book(
    store: &Store,
    op: &Operator,
    cemetery_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    doc: &str,
    volume: &str,
) -> Result<(Vec<Page>, String, String)> {
    let c = cemetery(store, cemetery_id)?;
    let period = match (from, to) {
        (Some(f), Some(t)) => format!("{} – {}", hu_date(f), hu_date(t)),
        (Some(f), None) => format!("{} –", hu_date(f)),
        (None, Some(t)) => format!("– {}", hu_date(t)),
        (None, None) => "teljes".into(),
    };
    let mut pages_all: Vec<Page> = vec![];
    let mut digest = String::new();
    let parts: Vec<&str> = match doc {
        "inspection" => vec!["register", "crypt_book"],
        d => vec![d],
    };
    let title = match doc {
        "register" => "Nyilvántartó könyv".to_string(),
        "crypt_book" => "Sírbolt könyv".to_string(),
        _ => "Ellenőrzési export – nyilvántartó és sírbolt könyv".to_string(),
    };
    for part in parts {
        let part_title = if part == "register" { "Nyilvántartó könyv" } else { "Sírbolt könyv" };
        let header_text = format!("{part_title} · {} · időszak: {period} · kötet: {volume}", c.name);
        let mut l = Layout::a4(true).with_header(move |p, m| {
            p.ops.push(Op::Text { x: m, y: m + 9.0, size: 9.0, font: FontKind::Sans, text: header_text.clone(), color: GREY });
            16.0
        });
        // title block
        let m = l.margin;
        l.text(m, 20.0, FontKind::SerifBold, part_title, BLACK);
        l.gap(28.0);
        l.fields(
            &[
                ("Temető", format!("{}, {}", c.name, c.address)),
                ("Helyrajzi szám", c.parcel_number.clone()),
                ("Üzemeltető", if c.operator.is_empty() { op.name.clone() } else { c.operator.clone() }),
                ("Időszak", period.clone()),
                ("Kötet azonosító", volume.to_string()),
            ],
            10.0,
        );
        l.gap(8.0);
        let filter = EventFilter {
            cemetery_id: Some(cemetery_id.into()),
            date_from: from.map(String::from),
            date_to: to.map(String::from),
            include_voided: true,
            crypts_only: part == "crypt_book",
            ..Default::default()
        };
        let events = store.list_events(&filter)?.items;
        let mut rows = vec![];
        for e in &events {
            let name = if e.unknown_identity {
                format!("Ismeretlen ({}, {})", e.authority_name, e.case_number)
            } else if e.birth_name.is_empty() {
                e.deceased_name.clone()
            } else {
                format!("{}\nszül.: {}", e.deceased_name, e.birth_name)
            };
            let place = match (&e.plot_identifier, e.crypt_slot_no) {
                (Some(p), Some(n)) => format!("{p}\n{n}. férőhely"),
                (Some(p), None) => p.clone(),
                (None, _) => e.destination.clone(),
            };
            let who = {
                let holder = holder_text(store, &e.plot_id)?;
                let arr = match (&e.arranger_name, &e.arranger_address) {
                    (Some(n), Some(a)) if !a.is_empty() => format!("{n}, {a}"),
                    (Some(n), _) => n.clone(),
                    _ => String::new(),
                };
                let mut v = vec![];
                if !arr.is_empty() {
                    v.push(format!("Eltemettető: {arr}"));
                }
                if !holder.is_empty() {
                    v.push(format!("Jogosult: {holder}"));
                }
                if !e.receiver_name.is_empty() {
                    v.push(format!("Átvevő: {}", e.receiver_name));
                }
                v.join("\n")
            };
            let mut note = event_hu(&e.event_type).to_string();
            if e.historical {
                note.push_str(", papír alapú bejegyzés");
            }
            if let Some(v) = &e.voided_at {
                note = format!("ÉRVÉNYTELENÍTVE ({}): {}", hu_date(v), e.void_reason.clone().unwrap_or_default());
            }
            let row = vec![
                format!("{}.", e.folio),
                hu_date(&e.event_date),
                name,
                format!("{}\n{}", od(&e.birth_date), e.mother_name),
                e.last_residence.clone(),
                od(&e.death_date),
                place,
                who,
                monument_text(store, &e.plot_id)?,
                note,
            ];
            digest.push_str(&row.join("\u{1f}"));
            digest.push('\n');
            rows.push(TableRow { cells: row, struck: e.voided_at.is_some(), bold_first: true });
        }
        let cols = [
            ("Folyó-\nszám", 0.6),
            ("Temetés napja", 0.9),
            ("Az elhalt családi és utóneve, születési neve", 1.6),
            ("Születési ideje, anyja neve", 1.3),
            ("Legutolsó lakóhelye", 1.4),
            ("Elhalálozás ideje", 0.85),
            ("Sírhelytábla / sor / hely", 0.85),
            ("Eltemettető, rendelkezési jogosult neve és lakcíme", 2.0),
            ("Síremlékre vonatkozó bejegyzések", 1.2),
            ("Megjegyzés", 1.1),
        ]
        .map(|(t, w)| Column { title: t.into(), weight: w });
        if rows.is_empty() {
            l.paragraph(10.0, FontKind::Sans, "Az időszakban nincs bejegyzés.", GREY);
        } else {
            table(&mut l, &cols, &rows, 8.0);
        }
        if part == "crypt_book" {
            crypt_details(store, &mut l, cemetery_id, &mut digest)?;
        }
        pages_all.extend(l.pages);
    }
    let hash = sha256_hex(digest.as_bytes());
    // authentication clause on a final page (JOG-04, FR-NYO-01)
    let total_pages = pages_all.len() + 1;
    let mut l = Layout::a4(true);
    let m = l.margin;
    l.text(m, 18.0, FontKind::SerifBold, "Hitelesítési záradék", BLACK);
    l.gap(30.0);
    l.paragraph(
        11.0,
        FontKind::Sans,
        &format!(
            "Hitelesítem, hogy ez a(z) {} {} a(z) {} időszakra vonatkozóan, folyószám-sorrendben, {} számozott oldalon (ezzel a záradéklappal együtt) tartalmazza az elektronikus nyilvántartás bejegyzéseit. Az érvénytelenített bejegyzések áthúzva, az érvénytelenítés okával szerepelnek; oldalt kivenni, megsemmisíteni tilos.",
            c.name,
            title.to_lowercase(),
            period,
            total_pages
        ),
        BLACK,
    );
    l.gap(8.0);
    l.fields(&[("Kötet azonosító", volume.to_string()), ("Tartalmi kivonat (SHA-256)", hash.clone()), ("Nyomtatva", crate::util::now_ts())], 10.0);
    if !op.regulation.is_empty() {
        l.paragraph(
            9.5,
            FontKind::Sans,
            &format!("Jogszabályi alap: 1999. évi XLIII. törvény 18. §, 145/1999. (X. 1.) Korm. rendelet 6. §, {}.", op.regulation),
            GREY,
        );
    }
    l.signature(&place_date(op), &op.signer, &op.signer_title);
    pages_all.extend(l.pages);
    let footer = format!("Kötet: {volume} · kivonat: {}…", &hash[..16]);
    add_footers(&mut pages_all, 14.0 * MM, &footer);
    Ok((pages_all, format!("{title} – {}", c.name), hash))
}

/// FR-SIR-01/02: per-crypt data and slot occupancy appended to the crypt book.
fn crypt_details(store: &Store, l: &mut Layout, cemetery_id: &str, digest: &mut String) -> Result<()> {
    let ids: Vec<String> = {
        let mut st = store.conn.prepare(
            "SELECT p.id FROM plots p JOIN plot_types t ON t.id=p.plot_type_id WHERE p.cemetery_id=?1 AND t.category='crypt' AND p.voided_at IS NULL ORDER BY p.identifier",
        )?;
        let rows = st.query_map([cemetery_id], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    if ids.is_empty() {
        return Ok(());
    }
    l.new_page();
    let m = l.margin;
    l.text(m, 14.0, FontKind::SerifBold, "Sírboltok és férőhelyek", BLACK);
    l.gap(24.0);
    for id in ids {
        let p = store.get_plot(&id)?;
        let holders = holder_text(store, &Some(id.clone()))?;
        l.ensure(80.0);
        let m = l.margin;
        l.text(m, 11.0, FontKind::SansBold, &format!("Sírbolt {} – {}", p.crypt_number.clone().unwrap_or_default(), p.identifier), BLACK);
        l.gap(16.0);
        l.fields(
            &[
                ("Férőhelyek", p.coffin_capacity.to_string()),
                ("Építés éve", p.crypt_built_year.map(|y| y.to_string()).unwrap_or_default()),
                ("Építési engedély / terv", p.crypt_permit.clone().unwrap_or_default()),
                ("Rendelkezési jogosult", holders.clone()),
                ("A sírboltba temetés rendje", p.crypt_holder_instructions.clone().unwrap_or_default()),
            ],
            9.0,
        );
        let slots = store.crypt_slots(&id)?;
        let rows: Vec<TableRow> = slots
            .iter()
            .map(|s| TableRow {
                cells: vec![
                    format!("{}.", s.slot_no),
                    s.occupant_name.clone().unwrap_or_else(|| "szabad".into()),
                    s.occupant_date.as_deref().map(hu_date).unwrap_or_default(),
                    s.occupant_folio.map(|f| format!("{f}.")).unwrap_or_default(),
                ],
                struck: false,
                bold_first: true,
            })
            .collect();
        for r in &rows {
            digest.push_str(&r.cells.join("\u{1f}"));
            digest.push('\n');
        }
        table(
            l,
            &[("Férőhely", 0.6), ("Nyugvó", 2.0), ("Elhelyezés napja", 1.0), ("Folyószám", 0.7)].map(|(t, w)| Column { title: t.into(), weight: w }),
            &rows,
            8.5,
        );
        l.gap(12.0);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Alphabetical index (FR-KER-03)
// ---------------------------------------------------------------------------

fn index_doc(store: &Store, op: &Operator, cemetery_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let c = cemetery(store, cemetery_id)?;
    let mut st = store.conn.prepare(
        "SELECT trim(d.name_prefix||' '||d.family_name||' '||d.given_name), d.birth_name, d.birth_date, d.death_date, p.identifier, e.folio
         FROM interment_events e JOIN deceased d ON d.id=e.deceased_id LEFT JOIN plots p ON p.id=e.plot_id
         WHERE e.cemetery_id=?1 AND e.voided_at IS NULL AND e.event_type IN ('coffin_burial','urn_placement','reburial','common_grave','ash_scattering','relocation')
         ORDER BY d.sort_key, d.death_date",
    )?;
    let rows: Vec<TableRow> = st
        .query_map([cemetery_id], |r| {
            let b: Option<String> = r.get(2)?;
            let dd: Option<String> = r.get(3)?;
            let bn: String = r.get(1)?;
            let name: String = r.get(0)?;
            Ok(TableRow {
                cells: vec![
                    if bn.is_empty() { name } else { format!("{name} (szül. {bn})") },
                    format!("{}–{}", b.as_deref().map(|x| &x[..4]).unwrap_or("?"), dd.as_deref().map(|x| &x[..4]).unwrap_or("?")),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    format!("{}.", r.get::<_, i64>(5)?),
                ],
                struck: false,
                bold_first: true,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, &format!("Betűsoros névmutató – {}", c.name));
    table(&mut l, &[("Név", 3.0), ("Évek", 1.1), ("Hely", 1.0), ("Folyószám", 0.8)].map(|(t, w)| Column { title: t.into(), weight: w }), &rows, 9.0);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &c.name);
    Ok((pages, format!("Névmutató – {}", c.name), None))
}

// ---------------------------------------------------------------------------
// Information sheet (FR-KER-04, JOG-10): no personal data of holders
// ---------------------------------------------------------------------------

fn information(store: &Store, op: &Operator, deceased_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let d = store.get_deceased(deceased_id)?;
    let ev = store.deceased_events(deceased_id)?;
    let place = ev.iter().rfind(|e| e.voided_at.is_none() && e.plot_id.is_some()).cloned();
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Felvilágosítás temetési helyről");
    let name = crate::text::full_name(&d.name_prefix, &d.family_name, &d.given_name);
    l.fields(
        &[
            ("Elhunyt neve", name.clone()),
            ("Élt", format!("{} – {}", d.birth_date.as_deref().map(|x| &x[..4]).unwrap_or("?"), d.death_date.as_deref().map(|x| &x[..4]).unwrap_or("?"))),
            ("Temető", place.as_ref().map(|p| p.cemetery_name.clone()).unwrap_or_default()),
            ("Temetési hely", place.as_ref().and_then(|p| p.plot_identifier.clone()).unwrap_or_else(|| "nem a temetőben nyugszik".into())),
        ],
        12.0,
    );
    if let Some(pid) = place.as_ref().and_then(|p| p.plot_id.clone()) {
        let plot = store.get_plot(&pid)?;
        l.gap(10.0);
        let m = l.margin;
        l.text(m, 11.0, FontKind::SansBold, "Térképrészlet", BLACK);
        l.gap(18.0);
        let w = l.content_width();
        let h = 110.0 * MM;
        map_excerpt(store, &mut l, &plot, w, h)?;
        l.gap(h + 8.0);
        l.paragraph(
            9.0,
            FontKind::Sans,
            "A keresett hely vastag kerettel jelölve. A parcellák, sorok és helyek számozása a helyszínen is fel van tüntetve.",
            GREY,
        );
    }
    l.gap(10.0);
    l.paragraph(9.0, FontKind::Sans, "A felvilágosítás személyes adatot (a rendelkezési jogosult adatait) nem tartalmaz.", GREY);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("Felvilágosítás – {name}"), None))
}

/// Small map around a plot: neighbours within the section.
fn map_excerpt(store: &Store, l: &mut Layout, plot: &Plot, w: f32, h: f32) -> Result<()> {
    let (x0, y0) = (l.margin, l.y);
    l.page().ops.push(Op::Rect { x: x0, y: y0, w, h, fill: Some(Color(247, 249, 245)), stroke: Some((LIGHT, 0.6)), rot: 0.0 });
    let r = 120.0f64;
    let cx = plot.map_x + plot.map_w / 2.0;
    let cy = plot.map_y + plot.map_h / 2.0;
    let mut st = store.conn.prepare(
        "SELECT identifier, map_x, map_y, map_w, map_h, map_rot, id FROM plots WHERE cemetery_id=?1 AND voided_at IS NULL AND map_x BETWEEN ?2 AND ?3 AND map_y BETWEEN ?4 AND ?5",
    )?;
    let near: Vec<(String, f64, f64, f64, f64, f64, String)> = st
        .query_map(params![plot.cemetery_id, cx - r, cx + r, cy - r * 0.6, cy + r * 0.6], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let scale = (w as f64 / (2.0 * r)).min(h as f64 / (1.2 * r)) as f32;
    let ox = x0 + w / 2.0 - (cx as f32) * scale;
    let oy = y0 + h / 2.0 - (cy as f32) * scale;
    for (ident, x, y, pw, ph, rot, id) in near {
        let target = id == plot.id;
        let rx = ox + x as f32 * scale;
        let ry = oy + y as f32 * scale;
        if rx < x0 || ry < y0 || rx + pw as f32 * scale > x0 + w || ry + ph as f32 * scale > y0 + h {
            continue;
        }
        l.page().ops.push(Op::Rect {
            x: rx,
            y: ry,
            w: pw as f32 * scale,
            h: ph as f32 * scale,
            fill: Some(if target { MOSS } else { WHITE }),
            stroke: Some(if target { (BLACK, 2.0) } else { (GREY, 0.4) }),
            rot: rot as f32,
        });
        let label = ident.rsplit('/').next().unwrap_or("").to_string();
        let fs = (pw as f32 * scale * 0.35).clamp(4.0, 9.0);
        let tw = text_width(FontKind::Sans, fs, &label);
        l.page().ops.push(Op::Text {
            x: rx + pw as f32 * scale / 2.0 - tw / 2.0,
            y: ry + ph as f32 * scale / 2.0 + fs / 3.0,
            size: fs,
            font: if target { FontKind::SansBold } else { FontKind::Sans },
            text: label,
            color: if target { WHITE } else { GREY },
        });
    }
    let t = format!("{} – a keresett hely", plot.identifier);
    l.page().ops.push(Op::Text { x: x0 + 6.0, y: y0 + h - 6.0, size: 9.0, font: FontKind::SansBold, text: t, color: BLACK });
    Ok(())
}

// ---------------------------------------------------------------------------
// Certificates and letters (FR-NYO-02)
// ---------------------------------------------------------------------------

fn lease_context(store: &Store, lease_id: &str) -> Result<(Lease, Plot, String, Option<Person>)> {
    let lease = store.get_lease(lease_id)?;
    let plot = store.get_plot(&lease.plot_id)?;
    let cname = store.get_cemetery(&plot.cemetery_id)?.name;
    let person = match &lease.person_id {
        Some(pid) => store.conn.query_row("SELECT * FROM persons WHERE id=?1", [pid], crate::store::map_person_pub).optional()?,
        None => None,
    };
    Ok((lease, plot, cname, person))
}

fn person_line(p: &Option<Person>) -> (String, String) {
    match p {
        Some(p) => (
            crate::text::full_name(&p.name_prefix, &p.family_name, &p.given_name),
            if p.mail_address.is_empty() { p.address.clone() } else { p.mail_address.clone() },
        ),
        None => ("–".into(), String::new()),
    }
}

fn lease_certificate(store: &Store, op: &Operator, lease_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let (lease, plot, cname, person) = lease_context(store, lease_id)?;
    let ptype = store.get_plot_type(&plot.plot_type_id)?;
    let title = match lease.kind.as_str() {
        "renewal" => "Újraváltási igazolás",
        "prepurchase" => "Igazolás temetési hely előre megváltásáról",
        _ => "Megváltási igazolás",
    };
    let (pname, paddr) = person_line(&person);
    let occupants = store.occupants(&plot.id)?.iter().map(|o| o.deceased_name.clone()).collect::<Vec<_>>().join(", ");
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, title);
    l.paragraph(11.0, FontKind::Sans, "Igazoljuk, hogy az alábbi temetési hely feletti rendelkezési jog megváltása (újraváltása) megtörtént.", BLACK);
    l.gap(8.0);
    l.fields(
        &[
            ("Temető", cname),
            ("Temetési hely", plot.identifier.clone()),
            ("Helytípus", ptype.name),
            ("Rendelkezési jogosult", pname),
            ("Lakcím", paddr),
            ("Nyugvók", occupants),
            ("Kezdete", hu_date(&lease.start_date)),
            ("Időtartam", format!("{} év", lease.duration_years)),
            ("Lejárat", hu_date(&lease.expiry_date)),
            ("Díj", if lease.fee_exempt { "díjmentes (nemzeti sírkert)".into() } else { huf(lease.fee_huf) }),
        ],
        11.0,
    );
    l.gap(8.0);
    l.paragraph(
        9.5,
        FontKind::Sans,
        "A rendelkezési jog a lejárat napján megszűnik, ha addig az újraváltás nem történik meg. A lejáratról az üzemeltető értesítést küld.",
        GREY,
    );
    l.signature(&place_date(op), &op.signer, &op.signer_title);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("{title} – {}", plot.identifier), None))
}

fn fee_request(store: &Store, op: &Operator, lease_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let (lease, plot, cname, person) = lease_context(store, lease_id)?;
    let (pname, paddr) = person_line(&person);
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Díjbekérő");
    l.fields(&[("Befizető", pname), ("Cím", paddr), ("Temető", cname), ("Temetési hely", plot.identifier.clone())], 11.0);
    l.gap(10.0);
    let kind = match lease.kind.as_str() {
        "renewal" => "Temetési hely újraváltási díja",
        _ => "Temetési hely megváltási díja",
    };
    table(
        &mut l,
        &[("Tétel", 3.0), ("Időszak", 2.0), ("Összeg", 1.0)].map(|(t, w)| Column { title: t.into(), weight: w }),
        &[TableRow {
            cells: vec![kind.into(), format!("{} – {}", hu_date(&lease.start_date), hu_date(&lease.expiry_date)), huf(lease.fee_huf)],
            struck: false,
            bold_first: false,
        }],
        10.0,
    );
    l.gap(10.0);
    l.fields(&[("Fizetendő összesen", huf(lease.fee_huf))], 12.0);
    l.gap(6.0);
    l.paragraph(9.5, FontKind::Sans, "Ez a díjbekérő nem számla. A számlát a befizetés után az üzemeltető számlázóprogramja állítja ki.", GREY);
    l.signature(&place_date(op), &op.signer, &op.signer_title);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("Díjbekérő – {}", plot.identifier), None))
}

/// FR-MEG-06: expiry letters, one page per plot (mail merge).
fn expiry_notice(store: &Store, s: &Session, op: &Operator, plot_ids: &[String]) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    if plot_ids.is_empty() {
        return Err(CoreError::validation("plotIds", "no plot selected"));
    }
    let mut pages = vec![];
    for pid in plot_ids {
        let detail = store.plot_detail(s, pid)?;
        let lease = detail.current_lease.clone();
        let holder = detail.holders.iter().find(|h| h.role == "primary" && h.end_date.is_none()).cloned();
        let mut l = Layout::a4(false);
        letterhead(&mut l, op, "Értesítés temetési hely megváltásának lejártáról");
        if let Some(h) = &holder {
            let m = l.margin;
            l.text(m, 11.0, FontKind::SansBold, &h.person_name, BLACK);
            l.gap(15.0);
            l.text(m, 11.0, FontKind::Sans, &h.person_address, BLACK);
            l.gap(28.0);
        }
        l.paragraph(11.0, FontKind::Sans, "Tisztelt Rendelkezési Jogosult!", BLACK);
        l.gap(6.0);
        let expiry = lease.as_ref().map(|l| hu_date(&l.expiry_date)).unwrap_or_default();
        let past = lease.as_ref().map(|l| l.expiry_date.as_str() < crate::util::fmt_date(today()).as_str()).unwrap_or(false);
        let occupants = detail.occupants.iter().map(|o| o.deceased_name.clone()).collect::<Vec<_>>().join(", ");
        l.paragraph(
            11.0,
            FontKind::Sans,
            &format!(
                "Értesítjük, hogy a(z) {} temető {} jelű temetési helyének ({}) használati ideje {} {}. Kérjük, hogy a rendelkezési jog meghosszabbítása (újraváltása) érdekében keresse fel ügyfélszolgálatunkat.",
                detail.cemetery_name,
                detail.plot.identifier,
                if occupants.is_empty() { detail.plot_type.name.clone() } else { format!("nyugszik: {occupants}") },
                expiry,
                if past { "napján lejárt" } else { "napján lejár" }
            ),
            BLACK,
        );
        l.gap(6.0);
        l.paragraph(
            11.0,
            FontKind::Sans,
            "Ha az újraváltás a helyi temetőrendeletben meghatározott türelmi időn belül nem történik meg, a temetési hely újra értékesíthetővé válik.",
            BLACK,
        );
        l.signature(&place_date(op), &op.signer, &op.signer_title);
        pages.extend(l.pages);
    }
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, "Lejárati értesítők".into(), None))
}

/// FR-MEG-06: warning labels for graves with QR code, two per A4 page.
fn grave_labels(store: &Store, op: &Operator, plot_ids: &[String]) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    if plot_ids.is_empty() {
        return Err(CoreError::validation("plotIds", "no plot selected"));
    }
    let (w, h) = Paper::A4.size();
    let mut pages = vec![];
    for chunk in plot_ids.chunks(2) {
        let mut page = Page { w, h, ops: vec![] };
        for (i, pid) in chunk.iter().enumerate() {
            let plot = store.get_plot(pid)?;
            let cname = store.get_cemetery(&plot.cemetery_id)?.name;
            let lease = store.leases_for_plot(pid)?.into_iter().find(|l| l.status == "active" && l.voided_at.is_none());
            let top = 15.0 * MM + i as f32 * (h / 2.0);
            let left = 15.0 * MM;
            let bw = w - 30.0 * MM;
            let bh = h / 2.0 - 25.0 * MM;
            page.ops.push(Op::Rect { x: left, y: top, w: bw, h: bh, fill: None, stroke: Some((BLACK, 1.5)), rot: 0.0 });
            page.ops.push(Op::Text { x: left + 10.0 * MM, y: top + 22.0 * MM, size: 26.0, font: FontKind::SerifBold, text: "FIGYELEM!".into(), color: BLACK });
            page.ops.push(Op::Text {
                x: left + 10.0 * MM,
                y: top + 36.0 * MM,
                size: 30.0,
                font: FontKind::SerifBold,
                text: plot.identifier.clone(),
                color: BLACK,
            });
            let msg = format!(
                "E temetési hely megváltási ideje {} lejár(t). Kérjük, jelentkezzen az üzemeltetőnél: {}{}.",
                lease.as_ref().map(|l| hu_date(&l.expiry_date)).unwrap_or_default(),
                op.name,
                if op.phone.is_empty() { String::new() } else { format!(", tel.: {}", op.phone) }
            );
            let mut y = top + 50.0 * MM;
            for line in wrap(FontKind::Sans, 14.0, &msg, bw - 70.0 * MM) {
                page.ops.push(Op::Text { x: left + 10.0 * MM, y, size: 14.0, font: FontKind::Sans, text: line, color: BLACK });
                y += 20.0;
            }
            page.ops.push(Op::Text { x: left + 10.0 * MM, y: top + bh - 8.0 * MM, size: 10.0, font: FontKind::Sans, text: cname, color: GREY });
            // QR code (FR-MEG-06, later scanned by the field app FR-KES-03)
            let payload = format!("LUNDA:{}:{}", plot.cemetery_id, plot.identifier);
            if let Ok(code) = qrcode::QrCode::new(payload.as_bytes()) {
                let n = code.width();
                let size = 50.0 * MM;
                let cell = size / n as f32;
                let qx = left + bw - size - 10.0 * MM;
                let qy = top + 15.0 * MM;
                for (idx, c) in code.to_colors().iter().enumerate() {
                    if *c == qrcode::Color::Dark {
                        let (cx, cy) = ((idx % n) as f32, (idx / n) as f32);
                        page.ops.push(Op::Rect {
                            x: qx + cx * cell,
                            y: qy + cy * cell,
                            w: cell + 0.05,
                            h: cell + 0.05,
                            fill: Some(BLACK),
                            stroke: None,
                            rot: 0.0,
                        });
                    }
                }
            }
        }
        pages.push(page);
    }
    Ok((pages, "Figyelmeztető címkék".into(), None))
}

fn notice_board(store: &Store, op: &Operator, plot_ids: &[String]) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let mut rows = vec![];
    for pid in plot_ids {
        let plot = store.get_plot(pid)?;
        let lease = store.leases_for_plot(pid)?.into_iter().find(|l| l.status == "active" && l.voided_at.is_none());
        let occ = store.occupants(pid)?.iter().map(|o| o.deceased_name.clone()).collect::<Vec<_>>().join(", ");
        rows.push(TableRow {
            cells: vec![plot.identifier.clone(), occ, lease.map(|l| hu_date(&l.expiry_date)).unwrap_or_default()],
            struck: false,
            bold_first: true,
        });
    }
    rows.sort_by_key(|r| crate::store::natural_key(&r.cells[0]));
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Hirdetmény – lejárt temetési helyek");
    l.paragraph(11.0, FontKind::Sans, "Az alábbi temetési helyek használati ideje lejárt vagy hamarosan lejár. Kérjük a rendelkezési jogosultakat, hogy az újraváltás érdekében keressék fel az üzemeltetőt.", BLACK);
    l.gap(8.0);
    table(&mut l, &[("Temetési hely", 1.0), ("Nyugvók", 3.0), ("Lejárat", 1.0)].map(|(t, w)| Column { title: t.into(), weight: w }), &rows, 10.0);
    l.gap(10.0);
    l.paragraph(10.0, FontKind::Sans, &format!("Kifüggesztve: {}", place_date(op)), BLACK);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, "Hirdetőtábla-lista".into(), None))
}

/// JOG-15: urn handover receipt.
fn urn_receipt(store: &Store, op: &Operator, event_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let e = store.get_event_row(event_id)?;
    let src = store.get_event(event_id)?;
    let d = store.get_deceased(&src.deceased_id)?;
    // straight after cremation there is no earlier place in the cemetery
    let from_plot = match &src.source_event_id {
        Some(sid) => format!("{}, {}", e.cemetery_name, store.get_event_row(sid)?.plot_identifier.unwrap_or_default()),
        None => "a hamvasztás után közvetlenül kiadva".to_string(),
    };
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Urna-átvételi elismervény és nyilatkozat");
    // Tv. 18. § (1a): identity data, last address, date of death, place, declarant
    l.fields(
        &[
            ("Folyószám", format!("{}.", e.folio)),
            ("Elhunyt neve", e.deceased_name.clone()),
            ("Születési név", if d.birth_name.is_empty() { "–".into() } else { d.birth_name.clone() }),
            ("Születési hely és idő", format!("{}, {}", d.birth_place, od(&d.birth_date))),
            ("Anyja neve", d.mother_name.clone()),
            ("Legutolsó lakóhely", d.last_residence.clone()),
            ("Elhalálozás ideje", od(&e.death_date)),
            ("Korábbi elhelyezés", from_plot),
            ("Kiadás napja", hu_date(&e.event_date)),
            ("Átvevő (nyilatkozó) neve", e.receiver_name.clone()),
            ("Átvevő lakcíme", src.receiver_address.clone()),
            ("A hamvak elhelyezésének helye", e.destination.clone()),
        ],
        11.0,
    );
    l.gap(8.0);
    l.paragraph(
        10.5,
        FontKind::Sans,
        "Alulírott átvevő aláírásommal igazolom, hogy a fenti elhunyt hamvait tartalmazó urnát sértetlen állapotban átvettem, és nyilatkozom, hogy a hamvakat a fent megjelölt helyen helyezem el. Tudomásul veszem, hogy a temetőn kívüli elhelyezés helyének megváltozását be kell jelentenem.",
        BLACK,
    );
    l.gap(4.0);
    l.paragraph(
        9.0,
        FontKind::Sans,
        "A nyilatkozatot az üzemeltető a temető fennállásáig megőrzi (a temetőkről és a temetkezésről szóló 1999. évi XLIII. törvény 18. § (8)).",
        GREY,
    );
    l.signature(&place_date(op), &e.receiver_name, "átvevő, nyilatkozó");
    l.signature("", &op.signer, &op.signer_title);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("Urna-átvételi elismervény – {}", e.deceased_name), None))
}

fn monument_permit(store: &Store, op: &Operator, monument_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let (plot_id, entry, desc, contractor, material, size, permit): (String, String, String, String, String, String, Option<String>) = store.conn.query_row(
        "SELECT plot_id, entry_date, description, contractor, material, size, permit_date FROM monuments WHERE id=?1",
        [monument_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )?;
    let plot = store.get_plot(&plot_id)?;
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Síremlék-állítási engedély");
    l.fields(
        &[
            ("Temetési hely", plot.identifier.clone()),
            ("Bejegyzés napja", hu_date(&entry)),
            ("Munka leírása", desc),
            ("Kivitelező", contractor),
            ("Anyag", material),
            ("Méret", size),
            ("Engedély dátuma", od(&permit)),
        ],
        11.0,
    );
    l.gap(8.0);
    l.paragraph(10.0, FontKind::Sans, "A síremlék a temetési hely méretein belül, a temető rendjének megfelelően helyezhető el. A munkavégzést a kivitelező a temetőgondnoknál előzetesen bejelenti.", BLACK);
    l.signature(&place_date(op), &op.signer, &op.signer_title);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("Síremlék-engedély – {}", plot.identifier), None))
}

/// FR-ENG-01/04: the document of a case on the operator's letterhead. Which
/// one depends on the state of the case: acknowledgement, request for missing
/// documents, permit (decision), refusal, or the notice to a neglected plot.
fn case_document(store: &Store, op: &Operator, case_id: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    type Row = (String, String, String, String, Option<String>, String, Option<String>, String, String, Option<String>);
    let (number, kind, subject, applicant, plot, received, deadline, status, decision, contractor): Row = store
        .conn
        .query_row(
            "SELECT k.case_number, k.kind, k.subject, k.applicant_name, p.identifier, k.received_on, k.deadline, k.status, k.decision, c.name
             FROM cases k LEFT JOIN plots p ON p.id = k.plot_id LEFT JOIN contractors c ON c.id = k.contractor_id
             WHERE k.id = ?1 AND k.voided_at IS NULL",
            [case_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::not_found("case", case_id))?;
    let kind_hu = match kind.as_str() {
        "lease_contract" => "megváltási szerződés",
        "renewal" => "újraváltás",
        "monument_permit" => "síremlék-állítási engedély",
        "monument_removal" => "síremlék elszállítása",
        "exhumation" => "exhumálás",
        "relocation" => "áthelyezés",
        "ash_release" => "hamvak kiadása",
        "holder_transfer" => "rendelkezési jog átruházása",
        "neglect" => "gondozatlan temetési hely",
        "complaint" => "panasz",
        _ => "egyéb ügy",
    };
    let (title, body): (String, String) = match (kind.as_str(), status.as_str()) {
        ("neglect", "received" | "missing_docs") => (
            "Felszólítás a temetési hely rendbetételére".into(),
            format!(
                "A temetőbejárás során megállapítottuk, hogy a(z) {} jelű temetési hely gondozatlan. Felhívjuk, hogy a temetési helyet {} napig tegye rendbe. \
                 Ha a felszólításnak nem tesz eleget, a temető rendje szerint intézkedünk.",
                plot.clone().unwrap_or_default(),
                deadline.as_deref().map(hu_date).unwrap_or_else(|| "a megadott határidő".into())
            ),
        ),
        (_, "approved") => (format!("Határozat – {kind_hu}"), "Kérelmét megvizsgáltuk, és az alábbiak szerint engedélyezzük.".into()),
        (_, "rejected") => (format!("Elutasító határozat – {kind_hu}"), "Kérelmét megvizsgáltuk, és az alábbi indokkal elutasítjuk.".into()),
        (_, "missing_docs") => (
            "Hiánypótlási felhívás".into(),
            format!(
                "Kérelmét átvettük, de az elbíráláshoz az alábbiak hiányoznak. Kérjük, pótolja őket {} napig.",
                deadline.as_deref().map(hu_date).unwrap_or_else(|| "15".into())
            ),
        ),
        (_, "closed") => (format!("Értesítés az ügy lezárásáról – {kind_hu}"), "Az ügyet lezártuk.".into()),
        _ => ("Átvételi elismervény".into(), format!("Kérelmét {} napon átvettük és iktattuk. Az ügyintézésről értesítjük.", hu_date(&received))),
    };
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, &title);
    let mut fields = vec![
        ("Iktatószám", number.clone()),
        ("Tárgy", subject),
        ("Ügyfél", if applicant.is_empty() { contractor.clone().unwrap_or_default() } else { applicant }),
    ];
    if let Some(p) = &plot {
        fields.push(("Temetési hely", p.clone()));
    }
    fields.push(("Beérkezett", hu_date(&received)));
    if let Some(d) = &deadline {
        fields.push(("Határidő", hu_date(d)));
    }
    l.fields(&fields, 11.0);
    l.gap(10.0);
    l.paragraph(10.5, FontKind::Sans, &body, BLACK);
    if !decision.trim().is_empty() {
        l.gap(6.0);
        l.paragraph(10.5, FontKind::SansBold, &decision, BLACK);
    }
    l.signature(&place_date(op), &op.signer, &op.signer_title);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, format!("{title} – {}", number.replace('/', "-")), None))
}

// ---------------------------------------------------------------------------
// Cemetery map (FR-TER-05) A4–A0, landscape
// ---------------------------------------------------------------------------

fn status_color(s: PlotStatus) -> Color {
    match s {
        PlotStatus::Free => Color(247, 249, 245),
        PlotStatus::Reserved => Color(110, 147, 191),
        PlotStatus::Occupied => Color(79, 122, 96),
        PlotStatus::Expired => Color(201, 150, 46),
        PlotStatus::Notified => Color(166, 90, 42),
        PlotStatus::Resellable => Color(125, 98, 160),
        PlotStatus::Liquidated => Color(124, 132, 134),
        PlotStatus::Unusable => Color(61, 68, 71),
    }
}

const STATUS_HU: &[(PlotStatus, &str)] = &[
    (PlotStatus::Free, "szabad"),
    (PlotStatus::Reserved, "lefoglalt"),
    (PlotStatus::Occupied, "foglalt"),
    (PlotStatus::Expired, "lejárt"),
    (PlotStatus::Notified, "értesítve"),
    (PlotStatus::Resellable, "újraértékesíthető"),
    (PlotStatus::Liquidated, "felszámolt"),
    (PlotStatus::Unusable, "nem használható"),
];

fn map_doc(store: &Store, op: &Operator, cemetery_id: &str, paper: Paper) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let c = cemetery(store, cemetery_id)?;
    let data = store.map_data(cemetery_id)?;
    let (pw, ph) = paper.size();
    let (w, h) = (ph, pw); // landscape
    let margin = 12.0 * MM;
    let k = w / Paper::A4.size().1; // scale typography with the paper
    let mut page = Page { w, h, ops: vec![] };
    page.ops.push(Op::Text {
        x: margin,
        y: margin + 18.0 * k,
        size: 18.0 * k,
        font: FontKind::SerifBold,
        text: format!("{} – helyszínrajz", c.name),
        color: BLACK,
    });
    page.ops.push(Op::Text {
        x: margin,
        y: margin + 32.0 * k,
        size: 9.0 * k,
        font: FontKind::Sans,
        text: format!("{} · {} · készült: {}", c.address, op.name, place_date(op)),
        color: GREY,
    });
    let top = margin + 58.0 * k;
    let legend_h = 18.0 * k;
    let area = (margin, top, w - 2.0 * margin, h - top - margin - legend_h);
    // bounds
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in &data.sections {
        x0 = x0.min(s.map_x as f32);
        y0 = y0.min(s.map_y as f32);
        x1 = x1.max((s.map_x + s.map_w) as f32);
        y1 = y1.max((s.map_y + s.map_h) as f32);
    }
    if x0 > x1 {
        x0 = 0.0;
        y0 = 0.0;
        x1 = 100.0;
        y1 = 100.0;
    }
    let scale = (area.2 / (x1 - x0)).min(area.3 / (y1 - y0));
    let ox = area.0 + (area.2 - (x1 - x0) * scale) / 2.0 - x0 * scale;
    let oy = area.1 + (area.3 - (y1 - y0) * scale) / 2.0 - y0 * scale;
    for s in &data.sections {
        page.ops.push(Op::Rect {
            x: ox + s.map_x as f32 * scale,
            y: oy + s.map_y as f32 * scale,
            w: s.map_w as f32 * scale,
            h: s.map_h as f32 * scale,
            fill: Some(WHITE),
            stroke: Some((LIGHT, 0.8 * k)),
            rot: s.map_rot as f32,
        });
        page.ops.push(Op::Text {
            x: ox + s.map_x as f32 * scale,
            y: oy + s.map_y as f32 * scale - 3.0 * k,
            size: 11.0 * k,
            font: FontKind::SerifBold,
            text: s.code.clone(),
            color: BLACK,
        });
    }
    let label = scale * 10.0 >= 12.0; // plot ≥ ~12 pt wide → numbers fit
    for (ident, _si, st, prot, x, y, pw, ph, rot) in &data.plots {
        let status = PlotStatus::ALL[*st as usize];
        let rx = ox + x * scale;
        let ry = oy + y * scale;
        page.ops.push(Op::Rect {
            x: rx,
            y: ry,
            w: pw * scale,
            h: ph * scale,
            fill: Some(status_color(status)),
            stroke: Some(if *prot == 1 { (Color(176, 141, 47), 1.2 * k) } else { (GREY, 0.25 * k) }),
            rot: *rot,
        });
        if label {
            let n = ident.rsplit('/').next().unwrap_or("").to_string();
            let fs = (pw * scale * 0.4).min(8.0 * k);
            let tw = text_width(FontKind::Sans, fs, &n);
            let dark = matches!(status, PlotStatus::Free | PlotStatus::Expired);
            page.ops.push(Op::Text {
                x: rx + pw * scale / 2.0 - tw / 2.0,
                y: ry + ph * scale / 2.0 + fs / 3.0,
                size: fs,
                font: FontKind::Sans,
                text: n,
                color: if dark { BLACK } else { WHITE },
            });
        }
    }
    // legend
    let mut lx = margin;
    let ly = h - margin - 6.0 * k;
    for (st, name) in STATUS_HU {
        page.ops.push(Op::Rect { x: lx, y: ly - 8.0 * k, w: 10.0 * k, h: 10.0 * k, fill: Some(status_color(*st)), stroke: Some((GREY, 0.4 * k)), rot: 0.0 });
        page.ops.push(Op::Text { x: lx + 14.0 * k, y: ly, size: 9.0 * k, font: FontKind::Sans, text: (*name).into(), color: BLACK });
        lx += 14.0 * k + text_width(FontKind::Sans, 9.0 * k, name) + 16.0 * k;
    }
    page.ops.push(Op::Rect { x: lx, y: ly - 8.0 * k, w: 10.0 * k, h: 10.0 * k, fill: Some(WHITE), stroke: Some((Color(176, 141, 47), 1.5 * k)), rot: 0.0 });
    page.ops.push(Op::Text { x: lx + 14.0 * k, y: ly, size: 9.0 * k, font: FontKind::Sans, text: "védett sír".into(), color: BLACK });
    // scale bar: 10 m = 100 dm
    let bar = 100.0 * scale;
    let bx = w - margin - bar;
    page.ops.push(Op::Line { x1: bx, y1: ly, x2: bx + bar, y2: ly, color: BLACK, width: 1.5 * k });
    page.ops.push(Op::Text { x: bx, y: ly - 4.0 * k, size: 8.0 * k, font: FontKind::Sans, text: "10 m".into(), color: BLACK });
    Ok((vec![page], format!("Térkép – {}", c.name), None))
}

// ---------------------------------------------------------------------------
// Reports as PDF (FR-RIP-05)
// ---------------------------------------------------------------------------

fn report_doc(store: &Store, op: &Operator, doc: &str, cemetery_id: Option<&str>) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let mut l = Layout::a4(true);
    let title = if doc == "report_utilization" { "Kihasználtság parcellánként" } else { "Éves temetési statisztika" };
    letterhead(&mut l, op, title);
    if doc == "report_utilization" {
        let rows = store.utilization(cemetery_id)?;
        let mut cols = vec![
            Column { title: "Temető".into(), weight: 2.0 },
            Column { title: "Parcella".into(), weight: 1.0 },
            Column { title: "Összesen".into(), weight: 1.0 },
        ];
        for (_, n) in STATUS_HU {
            cols.push(Column { title: (*n).into(), weight: 1.0 });
        }
        let data: Vec<TableRow> = rows
            .iter()
            .map(|r| {
                let mut cells = vec![r.cemetery_name.clone(), r.section_code.clone(), r.total.to_string()];
                for (st, _) in STATUS_HU {
                    cells.push(r.by_status.get(st.as_str()).copied().unwrap_or(0).to_string());
                }
                TableRow { cells, struck: false, bold_first: false }
            })
            .collect();
        table(&mut l, &cols, &data, 9.0);
    } else {
        let rows = store.yearly_stats(cemetery_id)?;
        let mut cols = vec![Column { title: "Év".into(), weight: 0.8 }];
        for (_, n) in EVENT_HU {
            cols.push(Column { title: (*n).into(), weight: 1.0 });
        }
        cols.push(Column { title: "Összesen".into(), weight: 0.9 });
        let data: Vec<TableRow> = rows
            .iter()
            .map(|r| {
                let mut cells = vec![r.year.to_string()];
                for (k, _) in EVENT_HU {
                    cells.push(r.by_type.get(*k).copied().unwrap_or(0).to_string());
                }
                cells.push(r.total.to_string());
                TableRow { cells, struck: false, bold_first: true }
            })
            .collect();
        table(&mut l, &cols, &data, 9.0);
    }
    let mut pages = l.pages;
    add_footers(&mut pages, 14.0 * MM, &op.name);
    Ok((pages, title.into(), None))
}

/// Díjtáblázat: the fees in force for the notice board (A4 portrait), one
/// table per cemetery with net, VAT and gross.
fn fee_list_doc(store: &Store, op: &Operator, cemetery_id: Option<&str>) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let rows = store.current_fee_list(cemetery_id)?;
    let mut l = Layout::a4(false);
    let title = "Temetői díjak";
    letterhead(&mut l, op, title);
    l.paragraph(
        10.5,
        FontKind::Sans,
        &format!("A temetőben {} napján érvényes díjtételek a díjrendelet alapján.", hu_date(&crate::util::fmt_date(crate::util::today()))),
        BLACK,
    );
    l.gap(6.0);
    if rows.is_empty() {
        l.paragraph(11.0, FontKind::Sans, "Nincs érvényes díjtétel rögzítve.", BLACK);
    }
    let mut cems: Vec<(String, Vec<&Value>)> = vec![];
    for r in &rows {
        let c = r["cemetery"].as_str().unwrap_or("").to_string();
        match cems.last_mut() {
            Some((name, list)) if *name == c => list.push(r),
            _ => cems.push((c, vec![r])),
        }
    }
    let cols = [("Díjtétel", 2.3), ("Helytípus", 1.6), ("Időtartam", 1.15), ("Egység", 0.95), ("Nettó", 1.35), ("Áfa", 0.9), ("Bruttó", 1.35)]
        .map(|(t, w)| Column { title: t.into(), weight: w });
    let many = cems.len() > 1;
    for (name, list) in &cems {
        if many {
            l.ensure(40.0);
            l.gap(6.0);
            l.paragraph(12.0, FontKind::SansBold, name, BLACK);
            l.gap(2.0);
        }
        let data: Vec<TableRow> = list
            .iter()
            .map(|r| TableRow {
                cells: vec![
                    r["item"].as_str().unwrap_or("").into(),
                    r["plotType"].as_str().unwrap_or("minden helytípus").into(),
                    r["leaseYears"].as_i64().map(|y| format!("{y} év")).unwrap_or_default(),
                    r["unit"].as_str().unwrap_or("").into(),
                    huf(r["netHuf"].as_i64().unwrap_or(0)),
                    r["vat"].as_str().unwrap_or("").into(),
                    huf(r["grossHuf"].as_i64().unwrap_or(0)),
                ],
                struck: false,
                bold_first: true,
            })
            .collect();
        table(&mut l, &cols, &data, 9.5);
    }
    l.gap(10.0);
    l.paragraph(
        9.5,
        FontKind::Sans,
        "A díjakat az üzemeltető pénztárában vagy átutalással lehet megfizetni. Kérdés esetén forduljon az ügyfélszolgálathoz.",
        BLACK,
    );
    l.gap(4.0);
    l.paragraph(10.0, FontKind::Sans, &format!("Kifüggesztve: {}", place_date(op)), BLACK);
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok((pages, "Díjtáblázat".into(), None))
}

// ---------------------------------------------------------------- burial documents (pack)

/// What a burial needs on paper, gathered once.
struct BurialCtx {
    event_id: String,
    event_type: String,
    container: String,
    event_date: String,
    historical: bool,
    cemetery: String,
    plot: Option<Plot>,
    deceased: crate::model::Deceased,
    name: String,
    /// "2026-11-14T11:00" and the facility (chapel) of the booking, if any
    starts_at: Option<String>,
    facility: Option<String>,
    work: Option<(String, String)>,
    lease_id: Option<String>,
    lease_fee: i64,
}

const WEEKDAYS: [&str; 7] = ["hétfő", "kedd", "szerda", "csütörtök", "péntek", "szombat", "vasárnap"];

fn hu_datetime(starts_at: &str) -> String {
    let date = &starts_at[..10.min(starts_at.len())];
    let wd = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map(|d| WEEKDAYS[d.weekday().num_days_from_monday() as usize]).unwrap_or("");
    let time = starts_at.get(11..16).unwrap_or("");
    format!("{} {wd}{}", hu_date(date), if time.is_empty() { String::new() } else { format!(", {time} óra") })
}

/// Age at death in full years.
fn age_at_death(d: &crate::model::Deceased) -> Option<i32> {
    let b = chrono::NaiveDate::parse_from_str(d.birth_date.as_deref()?, "%Y-%m-%d").ok()?;
    let x = chrono::NaiveDate::parse_from_str(d.death_date.as_deref()?, "%Y-%m-%d").ok()?;
    let mut age = x.year() - b.year();
    if (x.month(), x.day()) < (b.month(), b.day()) {
        age -= 1;
    }
    Some(age).filter(|a| *a >= 0)
}

fn burial_ctx(store: &Store, event_id: &str) -> Result<BurialCtx> {
    let row: Option<(String, String, String, i64, String, Option<String>, String)> = store
        .conn
        .query_row(
            "SELECT e.event_type, e.container, e.event_date, e.historical, c.name, e.plot_id, e.deceased_id
             FROM interment_events e JOIN cemeteries c ON c.id = e.cemetery_id WHERE e.id = ?1 AND e.voided_at IS NULL",
            params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    let Some((event_type, container, event_date, historical, cemetery, plot_id, deceased_id)) = row else {
        return Err(CoreError::not_found("event", event_id));
    };
    let deceased = store.get_deceased(&deceased_id)?;
    let name = crate::text::full_name(&deceased.name_prefix, &deceased.family_name, &deceased.given_name);
    let plot = match &plot_id {
        Some(p) => Some(store.get_plot(p)?),
        None => None,
    };
    let booking: Option<(String, Option<String>, String)> = store
        .conn
        .query_row(
            "SELECT b.starts_at, f.name, b.id FROM bookings b LEFT JOIN facilities f ON f.id = b.facility_id WHERE b.event_id = ?1 AND b.status <> 'cancelled' ORDER BY b.starts_at LIMIT 1",
            params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let work: Option<(String, String)> = match &booking {
        Some((_, _, bid)) => store
            .conn
            .query_row(
                "SELECT assignee, details FROM work_orders WHERE booking_id = ?1 AND kind = 'grave_digging' AND status <> 'cancelled' LIMIT 1",
                params![bid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?,
        None => None,
    };
    // the lease of the plot made at this burial (or the one in force)
    let lease = match &plot_id {
        Some(p) => store.leases_for_plot(p)?.into_iter().filter(|l| l.voided_at.is_none()).max_by(|a, b| a.start_date.cmp(&b.start_date)),
        None => None,
    };
    Ok(BurialCtx {
        event_id: event_id.to_string(),
        event_type,
        container,
        event_date,
        historical: historical != 0,
        cemetery,
        plot,
        deceased,
        name,
        starts_at: booking.as_ref().map(|b| b.0.clone()),
        facility: booking.and_then(|b| b.1),
        work,
        lease_id: lease.as_ref().map(|l| l.id.clone()),
        lease_fee: lease.map(|l| l.fee_huf).unwrap_or(0),
    })
}

fn is_burial(t: &str) -> bool {
    matches!(t, "coffin_burial" | "urn_placement" | "common_grave" | "ash_scattering" | "reburial")
}

/// The documents that belong to this event, with whether they are usually needed.
pub fn burial_items(store: &Store, event_id: &str) -> Result<Value> {
    let c = burial_ctx(store, event_id)?;
    let mut items = vec![];
    let mut add = |key: &str, title: &str, default: bool| items.push(serde_json::json!({ "key": key, "title": title, "default": default }));
    let burial = is_burial(&c.event_type);
    if burial {
        add("mourning_card", "Gyászlap (ravatalozóba, hirdetőtáblára)", !c.historical);
        add("funeral_notice", "Temetési értesítő a temető hirdetőtáblájára", !c.historical && c.starts_at.is_some());
    }
    if c.plot.is_some() && matches!(c.event_type.as_str(), "coffin_burial" | "urn_placement" | "reburial" | "common_grave") {
        add("dig_sheet", "Sírásási munkalap térképrészlettel", !c.historical);
        add("grave_marker", "Ideiglenes sírjel (fejfatábla)", !c.historical);
    }
    if c.lease_id.is_some() {
        add("lease_certificate", "Megváltási igazolás", true);
        if c.lease_fee > 0 {
            add("fee_request", "Díjbekérő", true);
        }
    }
    if c.event_type == "urn_release" {
        add("urn_receipt", "Urna-átvételi elismervény és nyilatkozat", true);
    }
    if c.plot.is_some() {
        add("information", "Felvilágosítás a családnak (térképpel)", true);
    }
    Ok(serde_json::json!({ "name": c.name, "items": items }))
}

fn frame(page: &mut Page, inset: f32, width: f32) {
    page.ops.push(Op::Rect { x: inset, y: inset, w: page.w - 2.0 * inset, h: page.h - 2.0 * inset, fill: None, stroke: Some((BLACK, width)), rot: 0.0 });
}

fn centered(page: &mut Page, y: f32, size: f32, font: FontKind, text: &str, color: Color) {
    let w = text_width(font, size, text);
    page.ops.push(Op::Text { x: (page.w - w) / 2.0, y, size, font, text: text.into(), color });
}

/// Gyászlap: a mourning card for the chapel and the notice board (A4 landscape).
fn mourning_card(c: &BurialCtx, symbol: &str) -> Vec<Page> {
    let (pw, ph) = Paper::A4.size();
    let mut p = Page { w: ph, h: pw, ops: vec![] };
    frame(&mut p, 10.0 * MM, 4.0);
    frame(&mut p, 13.0 * MM, 0.8);
    let mut y = 34.0 * MM;
    if symbol == "cross" {
        let cx = p.w / 2.0;
        p.ops.push(Op::Rect { x: cx - 1.6 * MM, y: y - 12.0 * MM, w: 3.2 * MM, h: 20.0 * MM, fill: Some(BLACK), stroke: None, rot: 0.0 });
        p.ops.push(Op::Rect { x: cx - 6.0 * MM, y: y - 7.0 * MM, w: 12.0 * MM, h: 3.0 * MM, fill: Some(BLACK), stroke: None, rot: 0.0 });
        y += 14.0 * MM;
    }
    centered(&mut p, y, 15.0, FontKind::Serif, "Fájó szívvel tudatjuk, hogy", BLACK);
    y += 20.0 * MM;
    centered(&mut p, y, 40.0, FontKind::SerifBold, &c.name, BLACK);
    y += 14.0 * MM;
    let birth = c.deceased.birth_date.as_deref().map(|d| &d[..4]).unwrap_or("");
    let death = c.deceased.death_date.as_deref().map(|d| &d[..4]).unwrap_or("");
    if !birth.is_empty() || !death.is_empty() {
        centered(&mut p, y, 18.0, FontKind::Serif, &format!("{birth} – {death}"), BLACK);
        y += 12.0 * MM;
    }
    let lived = match age_at_death(&c.deceased) {
        Some(a) => format!("életének {}. évében elhunyt.", a + 1),
        None => "elhunyt.".to_string(),
    };
    centered(&mut p, y, 15.0, FontKind::Serif, &lived, BLACK);
    y += 18.0 * MM;
    let when = match &c.starts_at {
        Some(s) => format!("Búcsúztatása {}", hu_datetime(s)),
        None => format!("Temetése {}", hu_date(&c.event_date)),
    };
    let place = match &c.facility {
        Some(f) => format!("{}, {}.", c.cemetery, f.to_lowercase()),
        None => format!("{}.", c.cemetery),
    };
    centered(&mut p, y, 14.0, FontKind::Serif, &format!("{when},"), BLACK);
    y += 8.0 * MM;
    centered(&mut p, y, 14.0, FontKind::Serif, &place, BLACK);
    y += 20.0 * MM;
    centered(&mut p, y, 16.0, FontKind::SerifBold, "Emlékét megőrizzük.", BLACK);
    let bottom = p.h - 22.0 * MM;
    centered(&mut p, bottom, 13.0, FontKind::Serif, "A gyászoló család", BLACK);
    vec![p]
}

/// Temetési értesítő: today's funeral on the cemetery notice board (A4 portrait).
fn funeral_notice(op: &Operator, c: &BurialCtx) -> Vec<Page> {
    let (w, h) = Paper::A4.size();
    let mut p = Page { w, h, ops: vec![] };
    frame(&mut p, 12.0 * MM, 2.5);
    let mut y = 40.0 * MM;
    centered(&mut p, y, 30.0, FontKind::SerifBold, "TEMETÉS", BLACK);
    y += 24.0 * MM;
    centered(&mut p, y, 32.0, FontKind::SerifBold, &c.name, BLACK);
    y += 12.0 * MM;
    let years =
        format!("{} – {}", c.deceased.birth_date.as_deref().map(|d| &d[..4]).unwrap_or("?"), c.deceased.death_date.as_deref().map(|d| &d[..4]).unwrap_or("?"));
    centered(&mut p, y, 16.0, FontKind::Serif, &years, BLACK);
    y += 24.0 * MM;
    let when = c.starts_at.as_deref().map(hu_datetime).unwrap_or_else(|| hu_date(&c.event_date));
    centered(&mut p, y, 20.0, FontKind::SansBold, &when, BLACK);
    y += 12.0 * MM;
    if let Some(f) = &c.facility {
        centered(&mut p, y, 16.0, FontKind::Sans, f, BLACK);
        y += 10.0 * MM;
    }
    if let Some(pl) = &c.plot {
        centered(&mut p, y, 16.0, FontKind::Sans, &format!("Sírhely: {}", pl.identifier), BLACK);
    }
    centered(&mut p, h - 30.0 * MM, 11.0, FontKind::Sans, &c.cemetery, GREY);
    if !op.name.is_empty() {
        centered(&mut p, h - 24.0 * MM, 10.0, FontKind::Sans, &op.name, GREY);
    }
    vec![p]
}

/// Ideiglenes sírjel: the label for the temporary wooden marker (two per A4, cut in half).
fn grave_marker(c: &BurialCtx) -> Vec<Page> {
    let (w, h) = Paper::A4.size();
    let mut p = Page { w, h, ops: vec![] };
    for half in 0..2 {
        let top = half as f32 * h / 2.0;
        let (x, bw, bh) = (15.0 * MM, w - 30.0 * MM, h / 2.0 - 30.0 * MM);
        p.ops.push(Op::Rect { x, y: top + 15.0 * MM, w: bw, h: bh, fill: None, stroke: Some((BLACK, 1.6)), rot: 0.0 });
        let mid = |p: &mut Page, y: f32, size: f32, font: FontKind, t: &str| {
            let tw = text_width(font, size, t);
            p.ops.push(Op::Text { x: (w - tw) / 2.0, y, size, font, text: t.into(), color: BLACK });
        };
        mid(&mut p, top + 50.0 * MM, 34.0, FontKind::SerifBold, &c.name);
        let years = format!(
            "{} – {}",
            c.deceased.birth_date.as_deref().map(hu_date).unwrap_or_else(|| "?".into()),
            c.deceased.death_date.as_deref().map(hu_date).unwrap_or_else(|| "?".into())
        );
        mid(&mut p, top + 66.0 * MM, 18.0, FontKind::Serif, &years);
        if let Some(pl) = &c.plot {
            mid(&mut p, top + 90.0 * MM, 22.0, FontKind::SansBold, &pl.identifier);
        }
        mid(&mut p, top + 102.0 * MM, 11.0, FontKind::Sans, &c.cemetery);
        if half == 0 {
            p.ops.push(Op::Line { x1: 8.0 * MM, y1: h / 2.0, x2: w - 8.0 * MM, y2: h / 2.0, color: LIGHT, width: 0.5 });
        }
    }
    vec![p]
}

/// Sírásási munkalap: for the grave digger, with the map around the plot.
fn dig_sheet(store: &Store, op: &Operator, c: &BurialCtx) -> Result<Vec<Page>> {
    let Some(plot) = &c.plot else { return Err(CoreError::validation("plotId", "no plot for this event")) };
    let mut l = Layout::a4(false);
    letterhead(&mut l, op, "Sírásási munkalap");
    let occupied: i64 = store.conn.query_row(
        "SELECT count(*) FROM interment_events WHERE plot_id = ?1 AND voided_at IS NULL AND id <> ?2 AND event_type IN ('coffin_burial', 'urn_placement', 'reburial', 'common_grave')",
        params![plot.id, c.event_id],
        |r| r.get(0),
    )?;
    let what = match c.container.as_str() {
        "urn" => "urna",
        _ => "koporsó",
    };
    l.fields(
        &[
            ("Temető", c.cemetery.clone()),
            ("Temetési hely", plot.identifier.clone()),
            ("Elhunyt", c.name.clone()),
            ("Temetés ideje", c.starts_at.as_deref().map(hu_datetime).unwrap_or_else(|| hu_date(&c.event_date))),
            ("Ravatalozó", c.facility.clone().unwrap_or_default()),
            ("Elhelyezés", what.to_string()),
            ("Rátemetés", if occupied > 0 { format!("igen, a helyen már {occupied} elhelyezés van") } else { "nem, üres hely".to_string() }),
            ("Felelős", c.work.as_ref().map(|w| w.0.clone()).unwrap_or_default()),
            ("Megjegyzés", c.work.as_ref().map(|w| w.1.clone()).unwrap_or_default()),
        ],
        11.0,
    );
    l.gap(6.0);
    let m = l.margin;
    l.text(m, 11.0, FontKind::SansBold, "Térképrészlet", BLACK);
    l.gap(18.0);
    let (w, h) = (l.content_width(), 95.0 * MM);
    map_excerpt(store, &mut l, plot, w, h)?;
    l.gap(h + 8.0);
    l.fields(&[("Sírásás mélysége", "________ m".to_string()), ("Elkészült", "____ év ____ hó ____ nap  ____ óra".to_string())], 11.0);
    l.signature(&place_date(op), "", "a munkát végző aláírása");
    let mut pages = l.pages;
    add_footers(&mut pages, 18.0 * MM, &op.name);
    Ok(pages)
}

/// Everything chosen for one burial in one PDF.
fn burial_pack(store: &Store, op: &Operator, event_id: &str, items: &[String], symbol: &str) -> Result<(Vec<Page>, String, Option<(String, String)>)> {
    let c = burial_ctx(store, event_id)?;
    if items.is_empty() {
        return Err(CoreError::validation("items", "choose at least one document").with("code", "required"));
    }
    let mut pages = vec![];
    for it in items {
        let mut more = match it.as_str() {
            "mourning_card" => mourning_card(&c, symbol),
            "funeral_notice" => funeral_notice(op, &c),
            "grave_marker" => grave_marker(&c),
            "dig_sheet" => dig_sheet(store, op, &c)?,
            "lease_certificate" => match &c.lease_id {
                Some(id) => lease_certificate(store, op, id)?.0,
                None => continue,
            },
            "fee_request" => match &c.lease_id {
                Some(id) => fee_request(store, op, id)?.0,
                None => continue,
            },
            "urn_receipt" => urn_receipt(store, op, event_id)?.0,
            "information" => information(store, op, &c.deceased.id)?.0,
            other => return Err(CoreError::new("bad_request", format!("unknown document {other}"))),
        };
        pages.append(&mut more);
    }
    Ok((pages, format!("Temetési nyomtatványok – {}", c.name), None))
}
