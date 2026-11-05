//! Quick search (FR-KER-01): accent/case-insensitive, partial matches on
//! deceased and holder names, birth/death years and plot identifiers.

use crate::sql::{params_from_iter, OptionalExtension, Value};

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
                 WHERE p.voided_at IS NULL AND p.identifier ILIKE ?1 ESCAPE '\\' ORDER BY length(p.identifier), p.identifier LIMIT ?2",
            )?;
            let rows = st.query_map(crate::params![format!("{}%", like_escape(raw)), limit], |r| {
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

        // ---- deceased: trigram index narrows the candidates, the binary
        // sort key (Hungarian order) avoids a callback collation
        // The trigram index finds the candidates first (MATERIALIZED keeps the
        // planner from walking the whole name order and probing each row).
        let mut cand = String::new();
        let mut conds = vec!["d.voided_at IS NULL".to_string()];
        let mut args: Vec<Value> = vec![];
        if !words.is_empty() {
            let mut sub = vec!["si.kind='deceased'".to_string()];
            for w in &words {
                args.push(Value::Text(format!("%{}%", like_escape(w))));
                sub.push(format!("si.text LIKE ?{} ESCAPE '\\'", args.len()));
            }
            cand = format!("WITH cand AS MATERIALIZED (SELECT si.ref_id FROM search_index si WHERE {}) ", sub.join(" AND "));
            conds.push("d.id IN (SELECT ref_id FROM cand)".to_string());
        }
        for y in &years {
            args.push(Value::Text(y.to_string()));
            let i = args.len();
            conds.push(format!("(substr(d.birth_date,1,4)=?{i} OR substr(d.death_date,1,4)=?{i})"));
        }
        args.push(Value::Integer(limit));
        let sql = format!(
            "{cand}SELECT d.id, trim(d.name_prefix||' '||d.family_name||' '||d.given_name), d.birth_name, d.birth_date, d.death_date
             FROM deceased d WHERE {} ORDER BY d.sort_key, d.death_date LIMIT ?{}",
            conds.join(" AND "),
            args.len()
        );
        let mut st = self.conn.prepare_cached(&sql)?;
        let mut rows = st.query(params_from_iter(args.iter()))?;
        let mut deceased: Vec<SearchHit> = vec![];
        while let Some(r) = rows.next()? {
            let birth: Option<String> = r.get(3)?;
            let death: Option<String> = r.get(4)?;
            let birth_name: String = r.get(2)?;
            let years =
                format!("{}–{}", birth.as_deref().map(|b| &b[..4.min(b.len())]).unwrap_or("?"), death.as_deref().map(|b| &b[..4.min(b.len())]).unwrap_or("?"));
            let subtitle = if birth_name.is_empty() { years } else { format!("{years} · szül. {birth_name}") };
            deceased.push(SearchHit {
                kind: "deceased".into(),
                id: r.get(0)?,
                title: r.get(1)?,
                subtitle,
                plot_id: None,
                plot_identifier: None,
                cemetery_name: None,
            });
        }
        drop(rows);
        // resolve the resting place only for the returned rows
        let mut pst = self.conn.prepare_cached(
            "SELECT p.id, p.identifier, c.name FROM interment_events e JOIN plots p ON p.id=e.plot_id JOIN cemeteries c ON c.id=p.cemetery_id
             WHERE e.deceased_id=?1 AND e.voided_at IS NULL ORDER BY e.event_date DESC, e.folio DESC LIMIT 1",
        )?;
        for mut h in deceased {
            if let Some((pid, ident, cname)) =
                pst.query_row([&h.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).optional()?
            {
                h.plot_id = Some(pid);
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
                "WITH cand AS MATERIALIZED (SELECT si.ref_id FROM search_index si WHERE {})
                 SELECT pe.id, trim(pe.name_prefix||' '||pe.family_name||' '||pe.given_name),
                        (SELECT count(*) FROM plot_holders h WHERE h.person_id=pe.id AND h.end_date IS NULL AND h.voided_at IS NULL)
                 FROM persons pe WHERE pe.id IN (SELECT ref_id FROM cand) AND pe.voided_at IS NULL
                 ORDER BY pe.sort_key LIMIT ?{}",
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
