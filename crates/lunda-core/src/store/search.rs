//! Quick search (FR-KER-01): accent/case-insensitive, partial matches on
//! deceased and holder names, birth/death years and plot identifiers.

use rusqlite::{params_from_iter, types::Value};

use super::structure::like_escape;
use super::*;
use crate::auth::Session;
use crate::text::normalize;

impl Store {
    pub fn quick_search(&self, _s: &Session, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let raw = query.trim();
        if raw.is_empty() {
            return Ok(vec![]);
        }
        let limit = limit.clamp(1, 200);
        let mut hits = Vec::new();

        // plot identifiers: "A/3/12", "a/3", "B/"
        if raw.contains('/') {
            let mut st = self.conn.prepare_cached(
                "SELECT p.id, p.identifier, c.name, t.name FROM plots p JOIN cemeteries c ON c.id=p.cemetery_id JOIN plot_types t ON t.id=p.plot_type_id
                 WHERE p.voided_at IS NULL AND p.identifier LIKE ?1 ESCAPE '\\' COLLATE NOCASE ORDER BY length(p.identifier), p.identifier LIMIT ?2",
            )?;
            let rows = st.query_map(rusqlite::params![format!("{}%", like_escape(raw)), limit], |r| {
                Ok(SearchHit {
                    kind: "plot".into(),
                    id: r.get(0)?,
                    title: r.get(1)?,
                    subtitle: r.get(3)?,
                    plot_id: Some(r.get(0)?),
                    plot_identifier: Some(r.get(1)?),
                    cemetery_name: Some(r.get(2)?),
                })
            })?;
            for h in rows {
                hits.push(h?);
            }
            return Ok(hits);
        }

        let norm = normalize(raw);
        let mut years: Vec<i64> = vec![];
        let mut words: Vec<String> = vec![];
        for tok in norm.split(' ') {
            match tok.parse::<i64>() {
                Ok(y) if (1700..=2200).contains(&y) && tok.len() == 4 => years.push(y),
                _ if !tok.is_empty() => words.push(tok.to_string()),
                _ => {}
            }
        }
        if words.is_empty() && years.is_empty() {
            return Ok(hits);
        }

        // ---- deceased
        let mut conds = vec!["d.voided_at IS NULL".to_string()];
        let mut args: Vec<Value> = vec![];
        let mut from = "deceased d".to_string();
        if !words.is_empty() {
            // trigram index on search_index narrows the candidates
            let mut sub = vec!["si.kind='deceased'".to_string()];
            for w in &words {
                args.push(Value::Text(format!("%{}%", like_escape(w))));
                sub.push(format!("si.text LIKE ?{} ESCAPE '\\'", args.len()));
            }
            from = format!("(SELECT ref_id FROM search_index si WHERE {}) m JOIN deceased d ON d.id = m.ref_id", sub.join(" AND "));
        }
        for y in &years {
            args.push(Value::Text(y.to_string()));
            let i = args.len();
            conds.push(format!("(substr(d.birth_date,1,4)=?{i} OR substr(d.death_date,1,4)=?{i})"));
        }
        args.push(Value::Integer(limit));
        let sql = format!(
            "SELECT d.id, trim(d.name_prefix||' '||d.family_name||' '||d.given_name), d.birth_name, d.birth_date, d.death_date,
                    (SELECT e.plot_id FROM interment_events e WHERE e.deceased_id=d.id AND e.voided_at IS NULL AND e.plot_id IS NOT NULL ORDER BY e.event_date DESC LIMIT 1) AS plot_id
             FROM {from} WHERE {} ORDER BY d.family_name COLLATE HU, d.given_name COLLATE HU, d.death_date LIMIT ?{}",
            conds.join(" AND "),
            args.len()
        );
        let mut st = self.conn.prepare_cached(&sql)?;
        let mut rows = st.query(params_from_iter(args.iter()))?;
        let mut deceased: Vec<(SearchHit, Option<String>)> = vec![];
        while let Some(r) = rows.next()? {
            let birth: Option<String> = r.get(3)?;
            let death: Option<String> = r.get(4)?;
            let birth_name: String = r.get(2)?;
            let years = format!(
                "{}–{}",
                birth.as_deref().map(|b| &b[..4.min(b.len())]).unwrap_or("?"),
                death.as_deref().map(|b| &b[..4.min(b.len())]).unwrap_or("?")
            );
            let subtitle = if birth_name.is_empty() { years } else { format!("{years} · szül. {birth_name}") };
            let plot_id: Option<String> = r.get(5)?;
            deceased.push((
                SearchHit { kind: "deceased".into(), id: r.get(0)?, title: r.get(1)?, subtitle, plot_id: plot_id.clone(), plot_identifier: None, cemetery_name: None },
                plot_id,
            ));
        }
        drop(rows);
        let mut pst = self.conn.prepare_cached("SELECT p.identifier, c.name FROM plots p JOIN cemeteries c ON c.id=p.cemetery_id WHERE p.id=?1")?;
        for (mut h, pid) in deceased {
            if let Some(pid) = pid {
                let (ident, cname): (String, String) = pst.query_row([pid], |r| Ok((r.get(0)?, r.get(1)?)))?;
                h.plot_identifier = Some(ident);
                h.cemetery_name = Some(cname);
            }
            hits.push(h);
        }

        // ---- persons (holders) – only by name
        if !words.is_empty() && years.is_empty() {
            let mut args: Vec<Value> = vec![];
            let mut sub = vec!["si.kind='person'".to_string()];
            for w in &words {
                args.push(Value::Text(format!("%{}%", like_escape(w))));
                sub.push(format!("si.text LIKE ?{} ESCAPE '\\'", args.len()));
            }
            args.push(Value::Integer(limit));
            let sql = format!(
                "SELECT pe.id, trim(pe.name_prefix||' '||pe.family_name||' '||pe.given_name),
                        (SELECT count(*) FROM plot_holders h WHERE h.person_id=pe.id AND h.end_date IS NULL AND h.voided_at IS NULL)
                 FROM (SELECT ref_id FROM search_index si WHERE {}) m JOIN persons pe ON pe.id=m.ref_id
                 WHERE pe.voided_at IS NULL ORDER BY pe.family_name COLLATE HU, pe.given_name COLLATE HU LIMIT ?{}",
                sub.join(" AND "),
                args.len()
            );
            let mut st = self.conn.prepare_cached(&sql)?;
            let rows = st.query_map(params_from_iter(args.iter()), |r| {
                let n: i64 = r.get(2)?;
                Ok(SearchHit {
                    kind: "person".into(),
                    id: r.get(0)?,
                    title: r.get(1)?,
                    subtitle: format!("{n}"),
                    plot_id: None,
                    plot_identifier: None,
                    cemetery_name: None,
                })
            })?;
            for h in rows {
                hits.push(h?);
            }
        }
        Ok(hits)
    }
}
