//! Számlázz.hu Számla Agent: proformas (díjbekérő) and invoices (számla).
//!
//! Requests are `xmlszamla` documents posted as the multipart field
//! `action-xmlagentxmlfile` to https://www.szamlazz.hu/szamla/. With
//! `valaszVerzio=2` the answer is XML with the number and the PDF (base64).
//! Without SZAMLAZZ_AGENT_KEY nothing is sent: a local TESZT-… number is
//! returned so every flow can be tried before the account is connected.

use lunda_core::{CoreError, Result};

pub struct Party {
    pub name: String,
    pub tax_number: String,
    pub postcode: String,
    pub city: String,
    pub address: String,
    pub email: String,
}

pub struct Line {
    pub name: String,
    pub quantity: i64,
    pub unit: String,
    pub unit_net: i64,
    pub vat_percent: i64,
    /// a comment printed under the line (period etc.)
    pub comment: String,
}

pub struct Document {
    pub proforma: bool,
    pub issue_date: String,
    pub fulfilment_date: String,
    pub due_date: String,
    /// "Átutalás", "Bankkártya"
    pub payment_method: String,
    pub note: String,
    pub order_number: String,
    /// number of the paid proforma when this invoice settles it
    pub proforma_number: Option<String>,
    pub paid: bool,
    pub buyer: Party,
    pub lines: Vec<Line>,
}

pub struct Issued {
    pub number: String,
    pub pdf: Option<Vec<u8>>,
    pub log: String,
}

pub fn amounts(line: &Line) -> (i64, i64, i64) {
    let net = line.quantity * line.unit_net;
    let vat = (net as f64 * line.vat_percent as f64 / 100.0).round() as i64;
    (net, vat, net + vat)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn build_xml(key: &str, d: &Document) -> String {
    let mut x = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xmlszamla xmlns="http://www.szamlazz.hu/xmlszamla" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.szamlazz.hu/xmlszamla https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd">
"#,
    );
    x += &format!(
        "  <beallitasok>\n    <szamlaagentkulcs>{}</szamlaagentkulcs>\n    <eszamla>true</eszamla>\n    <szamlaLetoltes>true</szamlaLetoltes>\n    <valaszVerzio>2</valaszVerzio>\n  </beallitasok>\n",
        esc(key)
    );
    x += "  <fejlec>\n";
    x += &format!("    <keltDatum>{}</keltDatum>\n", d.issue_date);
    x += &format!("    <teljesitesDatum>{}</teljesitesDatum>\n", d.fulfilment_date);
    x += &format!("    <fizetesiHataridoDatum>{}</fizetesiHataridoDatum>\n", d.due_date);
    x += &format!("    <fizmod>{}</fizmod>\n", esc(&d.payment_method));
    x += "    <penznem>HUF</penznem>\n    <szamlaNyelve>hu</szamlaNyelve>\n";
    x += &format!("    <megjegyzes>{}</megjegyzes>\n", esc(&d.note));
    x += &format!("    <rendelesSzam>{}</rendelesSzam>\n", esc(&d.order_number));
    if let Some(p) = &d.proforma_number {
        x += &format!("    <dijbekeroSzamlaszam>{}</dijbekeroSzamlaszam>\n", esc(p));
    }
    if d.proforma {
        x += "    <dijbekero>true</dijbekero>\n";
    }
    if d.paid {
        x += "    <fizetve>true</fizetve>\n";
    }
    x += "  </fejlec>\n";
    x += "  <elado>\n    <emailReplyto>szamlazas@temisz.hu</emailReplyto>\n";
    x += &format!(
        "    <emailTargy>{}</emailTargy>\n    <emailSzoveg>{}</emailSzoveg>\n  </elado>\n",
        if d.proforma { "TEMISZ díjbekérő" } else { "TEMISZ számla" },
        esc("Mellékelten küldjük a TEMISZ előfizetés bizonylatát. Köszönjük, hogy a TEMISZ-t választotta.")
    );
    let b = &d.buyer;
    x += "  <vevo>\n";
    x += &format!(
        "    <nev>{}</nev>\n    <irsz>{}</irsz>\n    <telepules>{}</telepules>\n    <cim>{}</cim>\n    <email>{}</email>\n    <sendEmail>true</sendEmail>\n",
        esc(&b.name),
        esc(&b.postcode),
        esc(&b.city),
        esc(&b.address),
        esc(&b.email)
    );
    if !b.tax_number.trim().is_empty() {
        x += &format!("    <adoszam>{}</adoszam>\n", esc(b.tax_number.trim()));
    }
    x += "  </vevo>\n  <tetelek>\n";
    for l in &d.lines {
        let (net, vat, gross) = amounts(l);
        x += &format!(
            "    <tetel>\n      <megnevezes>{}</megnevezes>\n      <mennyiseg>{}</mennyiseg>\n      <mennyisegiEgyseg>{}</mennyisegiEgyseg>\n      <nettoEgysegar>{}</nettoEgysegar>\n      <afakulcs>{}</afakulcs>\n      <nettoErtek>{net}</nettoErtek>\n      <afaErtek>{vat}</afaErtek>\n      <bruttoErtek>{gross}</bruttoErtek>\n      <megjegyzes>{}</megjegyzes>\n    </tetel>\n",
            esc(&l.name),
            l.quantity,
            esc(&l.unit),
            l.unit_net,
            l.vat_percent,
            esc(&l.comment)
        );
    }
    x += "  </tetelek>\n</xmlszamla>\n";
    x
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let a = xml.find(&open)? + open.len();
    let b = xml[a..].find(&close)? + a;
    Some(&xml[a..b])
}

/// Issues the document. `seq` numbers local test documents.
pub fn issue(d: &Document, seq: i64) -> Result<Issued> {
    let Ok(key) = std::env::var("SZAMLAZZ_AGENT_KEY") else {
        let prefix = if d.proforma { "TESZT-D" } else { "TESZT-S" };
        return Ok(Issued { number: format!("{prefix}-{}-{seq:05}", &d.issue_date[..4]), pdf: None, log: "test mode: SZAMLAZZ_AGENT_KEY not set".into() });
    };
    let xml = build_xml(&key, d);
    let boundary = format!("----temisz{}", lunda_core::util::new_id().replace('-', ""));
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"action-xmlagentxmlfile\"; filename=\"temisz.xml\"\r\nContent-Type: text/xml\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(xml.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let resp = ureq::post("https://www.szamlazz.hu/szamla/")
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .send(&body[..])
        .map_err(|e| CoreError::new("invoicing_failed", e.to_string()))?;
    let header_number = resp.headers().get("szlahu_szamlaszam").and_then(|v| v.to_str().ok()).map(String::from);
    let header_error = resp.headers().get("szlahu_error").and_then(|v| v.to_str().ok()).map(String::from);
    let text = resp.into_body().read_to_string().map_err(|e| CoreError::new("invoicing_failed", e.to_string()))?;
    if let Some(err) = header_error.or_else(|| tag(&text, "hibauzenet").map(String::from)) {
        return Err(CoreError::new("invoicing_failed", err));
    }
    let number = header_number
        .or_else(|| tag(&text, "szamlaszam").map(String::from))
        .ok_or_else(|| CoreError::new("invoicing_failed", "no invoice number in the answer"))?;
    use base64::Engine;
    let pdf = tag(&text, "pdf").and_then(|b| base64::engine::general_purpose::STANDARD.decode(b.trim().replace(['\n', '\r'], "")).ok());
    Ok(Issued { number, pdf, log: "issued via Számla Agent".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(proforma: bool) -> Document {
        Document {
            proforma,
            issue_date: "2026-11-05".into(),
            fulfilment_date: "2026-11-05".into(),
            due_date: "2026-11-13".into(),
            payment_method: "Átutalás".into(),
            note: "TEMISZ előfizetés".into(),
            order_number: "T-1".into(),
            proforma_number: None,
            paid: false,
            buyer: Party {
                name: "Kő & Társa <Kft.>".into(),
                tax_number: "12345678-2-13".into(),
                postcode: "1111".into(),
                city: "Budapest".into(),
                address: "Fő utca 1.".into(),
                email: "a@b.hu".into(),
            },
            lines: vec![Line {
                name: "TEMISZ előfizetés".into(),
                quantity: 2,
                unit: "temető/hó".into(),
                unit_net: 45_000,
                vat_percent: 27,
                comment: String::new(),
            }],
        }
    }

    #[test]
    fn amounts_and_xml() {
        let d = doc(true);
        assert_eq!(amounts(&d.lines[0]), (90_000, 24_300, 114_300));
        let x = build_xml("KEY", &d);
        assert!(x.contains("<dijbekero>true</dijbekero>"));
        assert!(x.contains("<nev>Kő &amp; Társa &lt;Kft.&gt;</nev>"));
        assert!(x.contains("<bruttoErtek>114300</bruttoErtek>"));
        assert!(x.contains("<adoszam>12345678-2-13</adoszam>"));
        // header order required by the schema
        let pos = |t: &str| x.find(t).unwrap();
        assert!(pos("<keltDatum>") < pos("<teljesitesDatum>") && pos("<fizmod>") < pos("<penznem>") && pos("<rendelesSzam>") < pos("<dijbekero>"));
    }

    #[test]
    fn test_mode_numbers() {
        std::env::remove_var("SZAMLAZZ_AGENT_KEY");
        let i = issue(&doc(false), 7).unwrap();
        assert_eq!(i.number, "TESZT-S-2026-00007");
        assert!(i.pdf.is_none());
    }
}
