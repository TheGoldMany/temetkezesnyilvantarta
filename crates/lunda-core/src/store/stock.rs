//! Goods and materials: items with a sale price, goods received, sales at
//! the cash desk or for invoicing, use on work, and stocktakes. The stock on
//! hand is the sum of the movements; a sale that would go below zero is refused.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::finance::{check_vat, vat_of, ChargeInput};
use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::sql::{params, OptionalExtension};
use crate::util::{fmt_date, new_id, now_ts, parse_date, today};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StockItem {
    #[serde(default)]
    pub id: String,
    pub cemetery_id: String,
    #[serde(default)]
    pub sku: String,
    pub name: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub unit_net_huf: i64,
    #[serde(default)]
    pub vat_rate: String,
    #[serde(default)]
    pub min_qty: i64,
    #[serde(default = "yes")]
    pub for_sale: bool,
    #[serde(default = "yes")]
    pub active: bool,
    /// read only: stock on hand, gross price, below the minimum
    #[serde(default)]
    pub on_hand: i64,
    #[serde(default)]
    pub unit_gross_huf: i64,
    #[serde(default)]
    pub low: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaleLine {
    pub item_id: String,
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaleInput {
    pub cemetery_id: String,
    pub payer_name: String,
    pub person_id: Option<String>,
    pub lines: Vec<SaleLine>,
    /// cash: receipt at the cash desk now; invoice: items to invoice
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub item_id: String,
    pub counted: i64,
}

fn can_manage(s: &Session) -> Result<()> {
    if s.can(Permission::RecordPayments) || s.can(Permission::CashDesk) || s.can(Permission::ManageSettings) {
        Ok(())
    } else {
        Err(CoreError::forbidden("record_payments"))
    }
}

impl Store {
    fn map_item(r: &crate::sql::Row) -> crate::sql::Result<StockItem> {
        let net: i64 = r.get("unit_net_huf")?;
        let vat: String = r.get("vat_rate")?;
        let on_hand: i64 = r.get("on_hand")?;
        let min: i64 = r.get("min_qty")?;
        let active: bool = r.get("active")?;
        Ok(StockItem {
            id: r.get("id")?,
            cemetery_id: r.get("cemetery_id")?,
            sku: r.get("sku")?,
            name: r.get("name")?,
            unit: r.get("unit")?,
            unit_net_huf: net,
            unit_gross_huf: net + vat_of(net, &vat),
            vat_rate: vat,
            min_qty: min,
            for_sale: r.get("for_sale")?,
            active,
            on_hand,
            low: active && min > 0 && on_hand < min,
        })
    }

    const ITEM_SQL: &'static str = "SELECT i.*, coalesce((SELECT sum(qty) FROM stock_moves m WHERE m.item_id = i.id), 0) AS on_hand FROM stock_items i";

    pub fn list_stock(&self, cemetery_id: &str, include_inactive: bool) -> Result<Vec<StockItem>> {
        let sql = format!("{} WHERE i.cemetery_id = ?1 AND (i.active OR ?2) ORDER BY i.name COLLATE {}", Self::ITEM_SQL, crate::db::HU);
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(params![cemetery_id, include_inactive], Self::map_item)?;
        Ok(rows.collect::<crate::sql::Result<_>>()?)
    }

    pub fn get_stock_item(&self, id: &str) -> Result<StockItem> {
        self.conn
            .query_row(&format!("{} WHERE i.id = ?1", Self::ITEM_SQL), params![id], Self::map_item)
            .optional()?
            .ok_or_else(|| CoreError::not_found("stock_item", id))
    }

    /// Creates or updates an item (the stock itself only changes through movements).
    pub fn save_stock_item(&self, s: &Session, it: &StockItem) -> Result<StockItem> {
        can_manage(s)?;
        non_empty("cemeteryId", &it.cemetery_id)?;
        let name = it.name.trim();
        if name.is_empty() {
            return Err(CoreError::validation("name", "name required").with("code", "required"));
        }
        if it.unit_net_huf < 0 || it.min_qty < 0 {
            return Err(CoreError::validation("unitNetHuf", "must not be negative"));
        }
        let vat = check_vat(&it.vat_rate)?;
        let unit = if it.unit.trim().is_empty() { "db" } else { it.unit.trim() };
        let ts = now_ts();
        let id = if it.id.is_empty() {
            let id = new_id();
            self.conn.execute(
                "INSERT INTO stock_items (id, cemetery_id, sku, name, unit, unit_net_huf, vat_rate, min_qty, for_sale, active, created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?11,?12)",
                params![id, it.cemetery_id, it.sku.trim(), name, unit, it.unit_net_huf, vat, it.min_qty, it.for_sale, it.active, ts, s.user_name],
            )?;
            audit::record_simple(&self.conn, s, "stock_item", &id, "create", Some(name))?;
            id
        } else {
            let n = self.conn.execute(
                "UPDATE stock_items SET sku=?2, name=?3, unit=?4, unit_net_huf=?5, vat_rate=?6, min_qty=?7, for_sale=?8, active=?9, updated_at=?10, updated_by=?11 WHERE id=?1",
                params![it.id, it.sku.trim(), name, unit, it.unit_net_huf, vat, it.min_qty, it.for_sale, it.active, ts, s.user_name],
            )?;
            if n == 0 {
                return Err(CoreError::not_found("stock_item", &it.id));
            }
            audit::record_simple(&self.conn, s, "stock_item", &it.id, "update", Some(name))?;
            it.id.clone()
        };
        self.get_stock_item(&id)
    }

    fn insert_move(
        &self,
        s: &Session,
        item: &StockItem,
        kind: &str,
        qty: i64,
        prices: (Option<i64>, Option<i64>),
        partner: &str,
        note: &str,
        links: (Option<&str>, Option<&str>, Option<&str>),
    ) -> Result<String> {
        let id = new_id();
        self.conn.execute(
            "INSERT INTO stock_moves (id, item_id, cemetery_id, kind, qty, unit_cost_huf, unit_net_huf, partner, note, cash_entry_id, charge_id, stocktake_id, moved_on, created_at, created_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![id, item.id, item.cemetery_id, kind, qty, prices.0, prices.1, partner, note, links.0, links.1, links.2, fmt_date(today()), now_ts(), s.user_name],
        )?;
        Ok(id)
    }

    /// Goods received (or material bought).
    pub fn stock_receive(&self, s: &Session, item_id: &str, qty: i64, unit_cost_huf: Option<i64>, partner: &str, note: &str) -> Result<StockItem> {
        can_manage(s)?;
        if qty <= 0 {
            return Err(CoreError::validation("qty", "quantity must be positive").with("code", "qty_invalid"));
        }
        let item = self.get_stock_item(item_id)?;
        let tx = crate::db::tx(&self.conn)?;
        self.insert_move(s, &item, "in", qty, (unit_cost_huf.filter(|c| *c >= 0), None), partner.trim(), note.trim(), (None, None, None))?;
        audit::record_simple(&tx, s, "stock_item", item_id, "stock_in", Some(&format!("+{qty} {}", item.unit)))?;
        tx.commit()?;
        self.get_stock_item(item_id)
    }

    /// Material used (repair, work order): stock goes down, nothing is sold.
    pub fn stock_use(&self, s: &Session, item_id: &str, qty: i64, note: &str) -> Result<StockItem> {
        if !(s.can(Permission::Operations) || s.can(Permission::CompleteWork) || s.can(Permission::RecordPayments)) {
            return Err(CoreError::forbidden("operations"));
        }
        if qty <= 0 {
            return Err(CoreError::validation("qty", "quantity must be positive").with("code", "qty_invalid"));
        }
        let item = self.get_stock_item(item_id)?;
        if item.on_hand < qty {
            return Err(CoreError::new("stock_short", "not enough in stock").with("name", item.name.clone()).with("onHand", item.on_hand));
        }
        let tx = crate::db::tx(&self.conn)?;
        self.insert_move(s, &item, "use", -qty, (None, None), "", note.trim(), (None, None, None))?;
        audit::record_simple(&tx, s, "stock_item", item_id, "stock_use", Some(&format!("-{qty} {}", item.unit)))?;
        tx.commit()?;
        self.get_stock_item(item_id)
    }

    /// Sells goods: a cash receipt now, or items to invoice. Refused when the
    /// stock is short. Returns the receipt or the charges made.
    pub fn stock_sell(&self, s: &Session, sale: &SaleInput) -> Result<Value> {
        can_manage(s)?;
        let payer = sale.payer_name.trim();
        if payer.is_empty() {
            return Err(CoreError::validation("payerName", "payer required").with("code", "required"));
        }
        if sale.lines.is_empty() || sale.lines.iter().any(|l| l.qty <= 0) {
            return Err(CoreError::validation("lines", "at least one item with a quantity").with("code", "qty_invalid"));
        }
        let mut items = Vec::new();
        for l in &sale.lines {
            let it = self.get_stock_item(&l.item_id)?;
            if it.cemetery_id != sale.cemetery_id || !it.active || !it.for_sale {
                return Err(CoreError::new("stock_not_for_sale", "this item is not for sale").with("name", it.name.clone()));
            }
            let wanted: i64 = sale.lines.iter().filter(|x| x.item_id == l.item_id).map(|x| x.qty).sum();
            if it.on_hand < wanted {
                return Err(CoreError::new("stock_short", "not enough in stock").with("name", it.name.clone()).with("onHand", it.on_hand));
            }
            items.push((it, l.qty));
        }
        let total: i64 = items.iter().map(|(it, q)| it.unit_gross_huf * q).sum();
        let tx = crate::db::tx(&self.conn)?;
        let out = match sale.method.as_str() {
            "cash" => {
                let what: Vec<String> = items.iter().map(|(it, q)| format!("{} {q} {}", it.name, it.unit)).collect();
                let date = fmt_date(today());
                let entry = self.insert_cash_entry(&tx, s, &sale.cemetery_id, &date, "in", total, payer, &format!("Áru: {}", what.join(", ")), None)?;
                for (it, q) in &items {
                    self.insert_move(s, it, "sale", -q, (None, Some(it.unit_net_huf)), payer, "", (Some(&entry.id), None, None))?;
                }
                json!({ "receipt": entry, "totalHuf": total })
            }
            "invoice" => {
                let mut charges = Vec::new();
                for (it, q) in &items {
                    let c = self.add_charge(
                        s,
                        &ChargeInput {
                            cemetery_id: sale.cemetery_id.clone(),
                            person_id: sale.person_id.clone(),
                            payer_name: Some(payer.to_string()),
                            description: Some(it.name.clone()),
                            quantity: Some(*q),
                            unit: Some(it.unit.clone()),
                            unit_net_huf: Some(it.unit_net_huf),
                            vat_rate: Some(it.vat_rate.clone()),
                            entity: Some("stock".into()),
                            entity_id: Some(it.id.clone()),
                            ..Default::default()
                        },
                    )?;
                    self.insert_move(s, it, "sale", -q, (None, Some(it.unit_net_huf)), payer, "", (None, Some(&c.id), None))?;
                    charges.push(c);
                }
                json!({ "charges": charges, "totalHuf": total })
            }
            _ => return Err(CoreError::validation("method", "cash or invoice").with("code", "required")),
        };
        audit::record_simple(&tx, s, "stock", &sale.cemetery_id, "sale", Some(&format!("{payer}, {total} Ft, {}", sale.method)))?;
        tx.commit()?;
        Ok(out)
    }

    /// The movements of an item, newest first.
    pub fn stock_moves(&self, item_id: &str) -> Result<Vec<Value>> {
        let text: String = self.conn.query_row(
            "SELECT coalesce(json_agg(json_build_object('id', id, 'kind', kind, 'qty', qty, 'unitCostHuf', unit_cost_huf, 'unitNetHuf', unit_net_huf,
                    'partner', partner, 'note', note, 'movedOn', moved_on, 'by', created_by) ORDER BY created_at DESC), '[]')::text
             FROM (SELECT * FROM stock_moves WHERE item_id = ?1 ORDER BY created_at DESC LIMIT 300) m",
            params![item_id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Stocktake: the counted quantities; every difference becomes an
    /// adjustment, and the count is kept with what the books said.
    pub fn stocktake(&self, s: &Session, cemetery_id: &str, counts: &[Count], note: &str, date: Option<&str>) -> Result<Value> {
        can_manage(s)?;
        if counts.is_empty() {
            return Err(CoreError::validation("counts", "count at least one item").with("code", "required"));
        }
        let taken_on = match date.filter(|d| !d.is_empty()) {
            Some(d) => fmt_date(parse_date(d)?),
            None => fmt_date(today()),
        };
        let tx = crate::db::tx(&self.conn)?;
        let id = new_id();
        let mut lines = Vec::new();
        let mut diffs = 0;
        for c in counts {
            if c.counted < 0 {
                return Err(CoreError::validation("counted", "must not be negative").with("code", "qty_invalid"));
            }
            let it = self.get_stock_item(&c.item_id)?;
            if it.cemetery_id != cemetery_id {
                return Err(CoreError::not_found("stock_item", &c.item_id));
            }
            let diff = c.counted - it.on_hand;
            if diff != 0 {
                self.insert_move(s, &it, "adjust", diff, (None, None), "", &format!("Leltár {taken_on}"), (None, None, Some(&id)))?;
                diffs += 1;
            }
            lines.push(json!({ "itemId": it.id, "name": it.name, "unit": it.unit, "expected": it.on_hand, "counted": c.counted, "diff": diff }));
        }
        let ts = now_ts();
        tx.execute(
            "INSERT INTO stocktakes (id, cemetery_id, taken_on, note, lines, created_at, created_by) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, cemetery_id, taken_on, note.trim(), Value::Array(lines.clone()).to_string(), ts, s.user_name],
        )?;
        audit::record_simple(&tx, s, "stocktake", &id, "create", Some(&format!("{} tétel, {diffs} eltérés", lines.len())))?;
        tx.commit()?;
        Ok(json!({ "id": id, "takenOn": taken_on, "lines": lines, "differences": diffs }))
    }

    pub fn stocktakes(&self, cemetery_id: &str) -> Result<Vec<Value>> {
        let mut st = self
            .conn
            .prepare("SELECT id, taken_on, note, lines, created_by FROM stocktakes WHERE cemetery_id = ?1 ORDER BY taken_on DESC, created_at DESC LIMIT 50")?;
        let rows = st.query_map(params![cemetery_id], |r| {
            let lines: String = r.get(3)?;
            Ok(json!({ "id": r.get::<_, String>(0)?, "takenOn": r.get::<_, String>(1)?, "note": r.get::<_, String>(2)?,
                       "lines": serde_json::from_str::<Value>(&lines).unwrap_or(json!([])), "by": r.get::<_, Option<String>>(4)? }))
        })?;
        Ok(rows.collect::<crate::sql::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Store, Session, String) {
        let store = Store::in_memory().unwrap();
        let s = Session::system();
        crate::sample::load_sample(&store, &s).unwrap();
        let cem: String = store.conn.query_row("SELECT id FROM cemeteries LIMIT 1", [], |r| r.get(0)).unwrap();
        (store, s, cem)
    }

    fn candle(store: &Store, s: &Session, cem: &str) -> StockItem {
        store
            .save_stock_item(
                s,
                &StockItem {
                    cemetery_id: cem.into(),
                    sku: "MEC-01".into(),
                    name: "Mécses".into(),
                    unit_net_huf: 400,
                    vat_rate: "27".into(),
                    min_qty: 10,
                    for_sale: true,
                    active: true,
                    ..Default::default()
                },
            )
            .unwrap()
    }

    #[test]
    fn receive_sell_and_count() {
        let (store, s, cem) = setup();
        let it = candle(&store, &s, &cem);
        assert_eq!((it.on_hand, it.unit_gross_huf, it.low), (0, 508, true));
        // nothing to sell yet
        let sale = |q: i64, method: &str| SaleInput {
            cemetery_id: cem.clone(),
            payer_name: "Kiss Margit".into(),
            lines: vec![SaleLine { item_id: it.id.clone(), qty: q }],
            method: method.into(),
            ..Default::default()
        };
        assert_eq!(store.stock_sell(&s, &sale(1, "cash")).unwrap_err().code, "stock_short");
        let it2 = store.stock_receive(&s, &it.id, 50, Some(250), "Gyertya Kft.", "").unwrap();
        assert_eq!((it2.on_hand, it2.low), (50, false));
        // cash sale: a receipt for the gross total, stock down
        let r = store.stock_sell(&s, &sale(3, "cash")).unwrap();
        assert_eq!(r["totalHuf"], 1_524);
        assert!(r["receipt"]["docNumber"].as_str().unwrap().starts_with("BE-"));
        // for invoicing: a charge per line
        let r = store.stock_sell(&s, &sale(2, "invoice")).unwrap();
        assert_eq!(r["charges"].as_array().unwrap().len(), 1);
        assert_eq!(r["charges"][0]["grossHuf"], 1_016);
        // material use
        store.stock_use(&s, &it.id, 5, "sírhely helyreállítás").unwrap();
        assert_eq!(store.get_stock_item(&it.id).unwrap().on_hand, 40);
        assert_eq!(store.stock_use(&s, &it.id, 100, "").unwrap_err().code, "stock_short");
        // stocktake: 37 counted, the books said 40
        let st = store.stocktake(&s, &cem, &[Count { item_id: it.id.clone(), counted: 37 }], "év végi leltár", None).unwrap();
        assert_eq!((st["differences"].as_i64(), st["lines"][0]["diff"].as_i64()), (Some(1), Some(-3)));
        assert_eq!(store.get_stock_item(&it.id).unwrap().on_hand, 37);
        assert_eq!(store.stocktakes(&cem).unwrap().len(), 1);
        assert_eq!(store.stock_moves(&it.id).unwrap().len(), 5);
        assert!(crate::audit::verify_chain(&store.conn).unwrap().ok);
    }

    #[test]
    fn items_and_permissions() {
        let (store, s, cem) = setup();
        assert_eq!(store.save_stock_item(&s, &StockItem { cemetery_id: cem.clone(), name: " ".into(), ..Default::default() }).unwrap_err().code, "validation");
        let it = candle(&store, &s, &cem);
        let material = store
            .save_stock_item(
                &s,
                &StockItem { cemetery_id: cem.clone(), name: "Homok".into(), unit: "kg".into(), for_sale: false, active: true, ..Default::default() },
            )
            .unwrap();
        store.stock_receive(&s, &material.id, 500, None, "", "").unwrap();
        // material is used, not sold
        let sale = SaleInput {
            cemetery_id: cem.clone(),
            payer_name: "Valaki".into(),
            lines: vec![SaleLine { item_id: material.id.clone(), qty: 1 }],
            method: "cash".into(),
            ..Default::default()
        };
        assert_eq!(store.stock_sell(&s, &sale).unwrap_err().code, "stock_not_for_sale");
        // a reader cannot touch stock
        let reader = Session::for_role(Some("r".into()), "olvaso", "Olvasó", crate::auth::Role::Reader, &[]);
        assert_eq!(store.stock_receive(&reader, &it.id, 1, None, "", "").unwrap_err().code, "forbidden");
        assert_eq!(store.list_stock(&cem, false).unwrap().len(), 2);
    }
}
