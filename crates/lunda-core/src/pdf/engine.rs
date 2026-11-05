//! Small PDF layout engine on top of krilla.
//!
//! Documents are laid out into a display list per page first; rendering
//! happens at the end, so the total page count is known for "X. / Y oldal"
//! numbering (FR-NYO-01). Fonts are embedded (OFL licensed, see fonts/).

use std::sync::OnceLock;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::Document;

use crate::error::{CoreError, Result};

pub const MM: f32 = 72.0 / 25.4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontKind {
    Sans,
    SansBold,
    Serif,
    SerifBold,
}

static SANS: &[u8] = include_bytes!("../../fonts/AtkinsonHyperlegibleNext_400Regular.ttf");
static SANS_BOLD: &[u8] = include_bytes!("../../fonts/AtkinsonHyperlegibleNext_700Bold.ttf");
static SERIF: &[u8] = include_bytes!("../../fonts/Literata_400Regular.ttf");
static SERIF_BOLD: &[u8] = include_bytes!("../../fonts/Literata_600SemiBold.ttf");

fn bytes(k: FontKind) -> &'static [u8] {
    match k {
        FontKind::Sans => SANS,
        FontKind::SansBold => SANS_BOLD,
        FontKind::Serif => SERIF,
        FontKind::SerifBold => SERIF_BOLD,
    }
}

struct Metrics {
    upem: f32,
    advances: std::collections::HashMap<char, u16>,
    face_data: &'static [u8],
}

fn metrics(k: FontKind) -> &'static Metrics {
    static M: OnceLock<[Metrics; 4]> = OnceLock::new();
    let all = M.get_or_init(|| {
        [FontKind::Sans, FontKind::SansBold, FontKind::Serif, FontKind::SerifBold].map(|k| {
            let data = bytes(k);
            let face = ttf_parser::Face::parse(data, 0).expect("embedded font");
            Metrics { upem: face.units_per_em() as f32, advances: Default::default(), face_data: data }
        })
    });
    &all[k as usize]
}

/// Text width in points (glyph advances, no kerning – good enough for layout).
pub fn text_width(k: FontKind, size: f32, text: &str) -> f32 {
    let m = metrics(k);
    let face = ttf_parser::Face::parse(m.face_data, 0).expect("font");
    let _ = &m.advances;
    let mut w = 0u32;
    for ch in text.chars() {
        let adv = face.glyph_index(ch).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((m.upem * 0.5) as u16);
        w += adv as u32;
    }
    w as f32 * size / m.upem
}

/// Greedy word wrap. Very long words are broken by characters.
pub fn wrap(k: FontKind, size: f32, text: &str, max: f32) -> Vec<String> {
    let mut lines = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(k, size, &candidate) <= max {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if text_width(k, size, word) <= max {
                line = word.to_string();
            } else {
                let mut chunk = String::new();
                for ch in word.chars() {
                    chunk.push(ch);
                    if text_width(k, size, &chunk) > max {
                        chunk.pop();
                        lines.push(std::mem::take(&mut chunk));
                        chunk.push(ch);
                    }
                }
                line = chunk;
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub u8, pub u8, pub u8);
pub const BLACK: Color = Color(31, 42, 48);
pub const GREY: Color = Color(86, 101, 107);
pub const LIGHT: Color = Color(201, 208, 203);
pub const MOSS: Color = Color(47, 93, 69);
pub const WHITE: Color = Color(255, 255, 255);

#[derive(Clone, Debug)]
pub enum Op {
    Text { x: f32, y: f32, size: f32, font: FontKind, text: String, color: Color },
    Rect { x: f32, y: f32, w: f32, h: f32, fill: Option<Color>, stroke: Option<(Color, f32)>, rot: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, color: Color, width: f32 },
    Image { x: f32, y: f32, w: f32, h: f32, png: Vec<u8> },
}

#[derive(Clone, Debug)]
pub struct Page {
    pub w: f32,
    pub h: f32,
    pub ops: Vec<Op>,
}

#[derive(Clone, Copy, Debug)]
pub enum Paper {
    A4,
    A3,
    A2,
    A1,
    A0,
}

impl Paper {
    pub fn parse(s: &str) -> Paper {
        match s.to_uppercase().as_str() {
            "A3" => Paper::A3,
            "A2" => Paper::A2,
            "A1" => Paper::A1,
            "A0" => Paper::A0,
            _ => Paper::A4,
        }
    }
    /// portrait width/height in points
    pub fn size(self) -> (f32, f32) {
        let (w, h) = match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::A2 => (420.0, 594.0),
            Paper::A1 => (594.0, 841.0),
            Paper::A0 => (841.0, 1189.0),
        };
        (w * MM, h * MM)
    }
}

/// Builder with a cursor; adds pages as content flows.
pub struct Layout {
    pub pages: Vec<Page>,
    pub w: f32,
    pub h: f32,
    pub margin: f32,
    pub y: f32,
    /// extra space reserved at the bottom for the footer
    pub footer: f32,
    /// called on every new page to draw the running header; returns its height
    pub header: Option<Box<dyn Fn(&mut Page, f32) -> f32>>,
}

impl Layout {
    pub fn new(w: f32, h: f32, margin: f32) -> Self {
        let mut l = Layout { pages: vec![], w, h, margin, y: margin, footer: 14.0 * MM, header: None };
        l.new_page();
        l
    }

    pub fn a4(landscape: bool) -> Self {
        let (w, h) = Paper::A4.size();
        if landscape {
            Layout::new(h, w, 14.0 * MM)
        } else {
            Layout::new(w, h, 18.0 * MM)
        }
    }

    pub fn new_page(&mut self) {
        let mut p = Page { w: self.w, h: self.h, ops: vec![] };
        let mut y = self.margin;
        if let Some(h) = &self.header {
            y += h(&mut p, self.margin);
        }
        self.pages.push(p);
        self.y = y;
    }

    pub fn with_header(mut self, h: impl Fn(&mut Page, f32) -> f32 + 'static) -> Self {
        self.header = Some(Box::new(h));
        // redraw the first page with the header
        self.pages.clear();
        self.new_page();
        self
    }

    pub fn content_width(&self) -> f32 {
        self.w - 2.0 * self.margin
    }

    pub fn bottom(&self) -> f32 {
        self.h - self.margin - self.footer
    }

    pub fn ensure(&mut self, height: f32) {
        if self.y + height > self.bottom() {
            self.new_page();
        }
    }

    pub fn page(&mut self) -> &mut Page {
        self.pages.last_mut().unwrap()
    }

    pub fn text(&mut self, x: f32, size: f32, font: FontKind, text: &str, color: Color) {
        let y = self.y;
        self.page().ops.push(Op::Text { x, y: y + size, size, font, text: text.into(), color });
    }

    /// Paragraph across the content width; advances the cursor.
    pub fn paragraph(&mut self, size: f32, font: FontKind, text: &str, color: Color) {
        let lh = size * 1.45;
        for line in wrap(font, size, text, self.content_width()) {
            self.ensure(lh);
            let x = self.margin;
            self.text(x, size, font, &line, color);
            self.y += lh;
        }
    }

    pub fn gap(&mut self, h: f32) {
        self.y += h;
    }

    pub fn hline(&mut self, color: Color, width: f32) {
        let (x1, x2, y) = (self.margin, self.w - self.margin, self.y);
        self.page().ops.push(Op::Line { x1, y1: y, x2, y2: y, color, width });
    }

    /// Key–value rows ("Elhunyt neve: …").
    pub fn fields(&mut self, rows: &[(&str, String)], size: f32) {
        let label_w = rows.iter().map(|(k, _)| text_width(FontKind::Sans, size, k)).fold(0.0, f32::max) + 10.0;
        let vx = self.margin + label_w;
        let vw = self.content_width() - label_w;
        for (k, v) in rows {
            let lines = wrap(FontKind::SansBold, size, if v.is_empty() { "–" } else { v }, vw);
            let h = lines.len() as f32 * size * 1.45;
            self.ensure(h);
            let m = self.margin;
            self.text(m, size, FontKind::Sans, k, GREY);
            let y0 = self.y;
            for (i, l) in lines.iter().enumerate() {
                self.y = y0 + i as f32 * size * 1.45;
                self.text(vx, size, FontKind::SansBold, l, BLACK);
            }
            self.y = y0 + h + 2.0;
        }
    }

    /// Signature line with caption, right aligned.
    pub fn signature(&mut self, place_date: &str, name: &str, title: &str) {
        self.ensure(70.0);
        self.gap(18.0);
        let m = self.margin;
        self.text(m, 10.0, FontKind::Sans, place_date, BLACK);
        self.gap(40.0);
        let x2 = self.w - self.margin;
        let x1 = x2 - 70.0 * MM;
        let y = self.y;
        self.page().ops.push(Op::Line { x1, y1: y, x2, y2: y, color: BLACK, width: 0.6 });
        self.gap(4.0);
        let cx = (x1 + x2) / 2.0;
        for (t, f) in [(name, FontKind::SansBold), (title, FontKind::Sans)] {
            if t.is_empty() {
                continue;
            }
            let w = text_width(f, 10.0, t);
            self.text(cx - w / 2.0, 10.0, f, t, BLACK);
            self.gap(13.0);
        }
        let ph = "P. H.";
        self.text(x1 - 30.0 * MM, 10.0, FontKind::Sans, ph, GREY);
    }
}

#[derive(Clone)]
pub struct Column {
    pub title: String,
    /// relative width weight
    pub weight: f32,
}

pub struct TableRow {
    pub cells: Vec<String>,
    /// draw a strike line through the row (voided register entries)
    pub struck: bool,
    pub bold_first: bool,
}

/// Table with header repeated on every page and wrapped cells.
pub fn table(l: &mut Layout, cols: &[Column], rows: &[TableRow], size: f32) {
    let total: f32 = cols.iter().map(|c| c.weight).sum();
    let widths: Vec<f32> = cols.iter().map(|c| l.content_width() * c.weight / total).collect();
    let pad = 3.0;
    let lh = size * 1.3;
    let draw_header = |l: &mut Layout| {
        let lines: Vec<Vec<String>> = cols.iter().zip(&widths).map(|(c, w)| wrap(FontKind::SansBold, size, &c.title, w - 2.0 * pad)).collect();
        let h = lines.iter().map(|v| v.len()).max().unwrap_or(1) as f32 * lh + 2.0 * pad;
        l.ensure(h + lh * 2.0);
        let (y0, mut x) = (l.y, l.margin);
        let w_all = l.content_width();
        let m = l.margin;
        l.page().ops.push(Op::Rect { x: m, y: y0, w: w_all, h, fill: Some(Color(237, 240, 235)), stroke: Some((LIGHT, 0.5)), rot: 0.0 });
        for (i, ls) in lines.iter().enumerate() {
            for (j, t) in ls.iter().enumerate() {
                l.page().ops.push(Op::Text { x: x + pad, y: y0 + pad + size + j as f32 * lh, size, font: FontKind::SansBold, text: t.clone(), color: BLACK });
            }
            x += widths[i];
        }
        l.y += h;
    };
    draw_header(l);
    for r in rows {
        let lines: Vec<Vec<String>> = r
            .cells
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(i, (c, w))| wrap(if i == 0 && r.bold_first { FontKind::SansBold } else { FontKind::Sans }, size, c, w - 2.0 * pad))
            .collect();
        let h = lines.iter().map(|v| v.len()).max().unwrap_or(1) as f32 * lh + 2.0 * pad;
        if l.y + h > l.bottom() {
            l.new_page();
            draw_header(l);
        }
        let (y0, mut x) = (l.y, l.margin);
        for (i, ls) in lines.iter().enumerate() {
            let font = if i == 0 && r.bold_first { FontKind::SansBold } else { FontKind::Sans };
            for (j, t) in ls.iter().enumerate() {
                l.page().ops.push(Op::Text { x: x + pad, y: y0 + pad + size + j as f32 * lh, size, font, text: t.clone(), color: if r.struck { GREY } else { BLACK } });
            }
            x += widths[i];
        }
        let m = l.margin;
        let w_all = l.content_width();
        l.page().ops.push(Op::Line { x1: m, y1: y0 + h, x2: m + w_all, y2: y0 + h, color: LIGHT, width: 0.5 });
        // column separators
        let mut cx = m;
        for w in &widths[..widths.len() - 1] {
            cx += w;
            l.page().ops.push(Op::Line { x1: cx, y1: y0, x2: cx, y2: y0 + h, color: LIGHT, width: 0.4 });
        }
        if r.struck {
            let ym = y0 + pad + size * 0.65;
            l.page().ops.push(Op::Line { x1: m, y1: ym, x2: m + w_all, y2: ym, color: BLACK, width: 0.7 });
        }
        l.y += h;
    }
}

/// Footer on every page: "X. oldal / összesen Y oldal" + optional left text.
pub fn add_footers(pages: &mut [Page], margin: f32, left: &str) {
    let n = pages.len();
    for (i, p) in pages.iter_mut().enumerate() {
        let y = p.h - margin + 4.0;
        let t = format!("{}. oldal / összesen {} oldal", i + 1, n);
        let w = text_width(FontKind::Sans, 8.5, &t);
        p.ops.push(Op::Line { x1: margin, y1: y - 10.0, x2: p.w - margin, y2: y - 10.0, color: LIGHT, width: 0.5 });
        p.ops.push(Op::Text { x: p.w - margin - w, y, size: 8.5, font: FontKind::Sans, text: t, color: GREY });
        if !left.is_empty() {
            p.ops.push(Op::Text { x: margin, y, size: 8.5, font: FontKind::Sans, text: left.to_string(), color: GREY });
        }
    }
}

fn c(color: Color) -> krilla::paint::Paint {
    rgb::Color::new(color.0, color.1, color.2).into()
}

/// Renders the display lists into PDF bytes.
pub fn render(pages: &[Page], title: &str) -> Result<Vec<u8>> {
    let fonts: Vec<Font> = [FontKind::Sans, FontKind::SansBold, FontKind::Serif, FontKind::SerifBold]
        .iter()
        .map(|k| Font::new(bytes(*k).to_vec().into(), 0).ok_or_else(|| CoreError::new("pdf", "font")))
        .collect::<Result<_>>()?;
    let mut doc = Document::new();
    doc.set_metadata(Metadata::new().title(title.to_string()).creator("Lunda Temető".to_string()).producer("Lunda Temető".to_string()));
    for p in pages {
        let mut page = doc.start_page_with(PageSettings::from_wh(p.w, p.h).ok_or_else(|| CoreError::new("pdf", "page size"))?);
        let mut s = page.surface();
        for op in &p.ops {
            match op {
                Op::Text { x, y, size, font, text, color } => {
                    if text.is_empty() {
                        continue;
                    }
                    s.set_fill(Some(Fill { paint: c(*color), opacity: NormalizedF32::ONE, rule: Default::default() }));
                    s.set_stroke(None);
                    s.draw_text(Point::from_xy(*x, *y), fonts[*font as usize].clone(), *size, text, false, TextDirection::Auto);
                }
                Op::Rect { x, y, w, h, fill, stroke, rot } => {
                    let Some(r) = Rect::from_xywh(*x, *y, w.max(0.01), h.max(0.01)) else { continue };
                    let mut pb = PathBuilder::new();
                    pb.push_rect(r);
                    let Some(path) = pb.finish() else { continue };
                    if *rot != 0.0 {
                        s.push_transform(&Transform::from_rotate_at(*rot, x + w / 2.0, y + h / 2.0));
                    }
                    s.set_fill(fill.map(|f| Fill { paint: c(f), opacity: NormalizedF32::ONE, rule: Default::default() }));
                    s.set_stroke(stroke.map(|(col, wd)| Stroke { paint: c(col), width: wd, ..Default::default() }));
                    s.draw_path(&path);
                    if *rot != 0.0 {
                        s.pop();
                    }
                }
                Op::Line { x1, y1, x2, y2, color, width } => {
                    let mut pb = PathBuilder::new();
                    pb.move_to(*x1, *y1);
                    pb.line_to(*x2, *y2);
                    let Some(path) = pb.finish() else { continue };
                    s.set_fill(None);
                    s.set_stroke(Some(Stroke { paint: c(*color), width: *width, ..Default::default() }));
                    s.draw_path(&path);
                }
                Op::Image { x, y, w, h, png } => {
                    let img = Image::from_png(png.clone().into(), true).or_else(|_| Image::from_jpeg(png.clone().into(), true));
                    if let (Ok(img), Some(size)) = (img, Size::from_wh(*w, *h)) {
                        s.push_transform(&Transform::from_translate(*x, *y));
                        s.draw_image(img, size);
                        s.pop();
                    }
                }
            }
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|e| CoreError::new("pdf", format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_wraps_hungarian_text() {
        let w = text_width(FontKind::Sans, 10.0, "Őrsi Űrhajós Ödön");
        assert!(w > 50.0 && w < 120.0, "{w}");
        let lines = wrap(FontKind::Sans, 10.0, "Kovács János Mintavölgy Kossuth Lajos utca 12. szám alatti lakos", 80.0);
        assert!(lines.len() > 2);
        assert!(lines.iter().all(|l| text_width(FontKind::Sans, 10.0, l) <= 80.0));
    }

    #[test]
    fn renders_pdf_with_page_numbers() {
        let mut l = Layout::a4(false);
        for i in 0..120 {
            l.paragraph(11.0, FontKind::Sans, &format!("{i}. sor – árvíztűrő tükörfúrógép ŐŰ"), BLACK);
        }
        let mut pages = l.pages;
        assert!(pages.len() >= 3);
        add_footers(&mut pages, 18.0 * MM, "teszt");
        let pdf = render(&pages, "Teszt").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 5000);
    }
}
