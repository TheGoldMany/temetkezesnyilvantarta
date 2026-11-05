//! Dashboard and MVP reports (FR-RIP-01, FR-RIP-02).

use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::*;
use crate::rules::PlotStatus;
use crate::util::{fmt_date, today};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UtilizationRow {
    pub cemetery_name: String,
    pub section_code: String,
    pub total: i64,
    pub by_status: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct YearStatRow {
    pub year: i32,
    pub by_type: BTreeMap<String, i64>,
    pub total: i64,
}

/// Compact map payload (FR-TER-03, NFR-TEL-02): one small array per plot
/// instead of an object – 50 000 plots stay a few megabytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapData {
    pub sections: Vec<crate::model::Section>,
    /// status names in index order
    pub statuses: Vec<String>,
    /// [identifier, sectionIndex, statusIndex, protected(0/1), x, y, w, h, rot]
    pub plots: Vec<(String, u32, u8, u8, f32, f32, f32, f32, f32)>,
}

impl Store {
    pub fn map_data(&self, cemetery_id: &str) -> Result<MapData> {
        let sections = self.list_sections(cemetery_id)?;
        let idx: std::collections::HashMap<&str, u32> = sections.iter().enumerate().map(|(i, s)| (s.id.as_str(), i as u32)).collect();
        let page = self.plot_summaries(&PlotFilter { cemetery_id: Some(cemetery_id.to_string()), light: true, ..Default::default() })?;
        let r = |v: f64| ((v * 10.0).round() / 10.0) as f32;
        let plots = page
            .items
            .iter()
            .map(|p| {
                (
                    p.identifier.clone(),
                    *idx.get(p.section_id.as_str()).unwrap_or(&0),
                    PlotStatus::ALL.iter().position(|s| *s == p.status).unwrap_or(0) as u8,
                    p.protected as u8,
                    r(p.x),
                    r(p.y),
                    r(p.w),
                    r(p.h),
                    r(p.rot),
                )
            })
            .collect();
        Ok(MapData { sections, statuses: PlotStatus::ALL.iter().map(|s| s.as_str().to_string()).collect(), plots })
    }

    pub fn dashboard(&self, cemetery_id: Option<&str>) -> Result<DashboardStats> {
        let plots = self.plot_summaries(&PlotFilter { cemetery_id: cemetery_id.map(String::from), ..Default::default() })?;
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for s in PlotStatus::ALL {
            counts.insert(s.as_str().into(), 0);
        }
        let t = today();
        let soon = fmt_date(t + chrono::Duration::days(90));
        let now = fmt_date(t);
        let mut expiring_90 = 0;
        let mut expired = 0;
        for p in &plots.items {
            *counts.entry(p.status.as_str().into()).or_default() += 1;
            if let Some(e) = &p.lease_expiry {
                if !p.protected || p.status != PlotStatus::Occupied {
                    if e.as_str() < now.as_str() {
                        expired += 1;
                    } else if e.as_str() <= soon.as_str() {
                        expiring_90 += 1;
                    }
                }
            }
        }
        let year = t.year().to_string();
        let events_this_year: i64 = self.conn.query_row(
            "SELECT count(*) FROM interment_events WHERE voided_at IS NULL AND substr(event_date,1,4)=?1 AND (?2 IS NULL OR cemetery_id=?2)",
            rusqlite::params![year, cemetery_id],
            |r| r.get(0),
        )?;
        let deceased_total: i64 = self.conn.query_row("SELECT count(*) FROM deceased WHERE voided_at IS NULL", [], |r| r.get(0))?;
        let recent = self.list_events(&EventFilter {
            cemetery_id: cemetery_id.map(String::from),
            limit: Some(8),
            order: Some("date_desc".into()),
            ..Default::default()
        })?;
        Ok(DashboardStats {
            plots_total: plots.total,
            by_status: counts.into_iter().collect(),
            events_this_year,
            expiring_90,
            expired,
            deceased_total,
            recent_events: recent.items,
        })
    }

    /// FR-RIP-01: utilisation per cemetery and section.
    pub fn utilization(&self, cemetery_id: Option<&str>) -> Result<Vec<UtilizationRow>> {
        let plots = self.plot_summaries(&PlotFilter { cemetery_id: cemetery_id.map(String::from), ..Default::default() })?;
        let mut sections: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut st = self.conn.prepare("SELECT s.id, c.name, s.code FROM sections s JOIN cemeteries c ON c.id=s.cemetery_id")?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, c, code) = r?;
            sections.insert(id, (c, code));
        }
        let mut agg: BTreeMap<(String, String), BTreeMap<String, i64>> = BTreeMap::new();
        for p in plots.items {
            let key = sections.get(&p.section_id).cloned().unwrap_or_default();
            *agg.entry(key).or_default().entry(p.status.as_str().into()).or_default() += 1;
        }
        let mut out: Vec<UtilizationRow> =
            agg.into_iter().map(|((c, s), m)| UtilizationRow { cemetery_name: c, section_code: s, total: m.values().sum(), by_status: m }).collect();
        out.sort_by(|a, b| crate::text::hu_compare(&a.cemetery_name, &b.cemetery_name).then_with(|| crate::text::hu_compare(&a.section_code, &b.section_code)));
        Ok(out)
    }

    /// FR-RIP-02: yearly statistics by event type.
    pub fn yearly_stats(&self, cemetery_id: Option<&str>) -> Result<Vec<YearStatRow>> {
        let mut st = self.conn.prepare(
            "SELECT CAST(substr(event_date,1,4) AS INTEGER) y, event_type, count(*) FROM interment_events
             WHERE voided_at IS NULL AND (?1 IS NULL OR cemetery_id=?1) GROUP BY y, event_type ORDER BY y DESC",
        )?;
        let mut map: BTreeMap<i32, BTreeMap<String, i64>> = BTreeMap::new();
        for r in st.query_map([cemetery_id], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (y, t, n) = r?;
            map.entry(y).or_default().insert(t, n);
        }
        Ok(map.into_iter().rev().map(|(year, by_type)| YearStatRow { year, total: by_type.values().sum(), by_type }).collect())
    }
}
