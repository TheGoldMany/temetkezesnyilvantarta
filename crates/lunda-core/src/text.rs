//! Hungarian text handling: accent folding for search and the Hungarian
//! alphabetical collation (NFR-HAS-01).

use std::cmp::Ordering;

/// Lower-cases and removes diacritics; collapses whitespace.
/// Used for accent- and case-insensitive search (FR-KER-01).
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for ch in s.chars() {
        let folded = fold_char(ch);
        for c in folded.chars() {
            if c.is_whitespace() {
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            } else {
                out.extend(c.to_lowercase());
                last_space = false;
            }
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn fold_char(ch: char) -> String {
    let s = match ch {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ă' | 'ą' => "a",
        'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ă' | 'Ą' => "a",
        'é' | 'è' | 'ê' | 'ë' | 'ě' | 'ę' => "e",
        'É' | 'È' | 'Ê' | 'Ë' | 'Ě' | 'Ę' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'Í' | 'Ì' | 'Î' | 'Ï' => "i",
        'ó' | 'ò' | 'ô' | 'ö' | 'ő' | 'õ' | 'ø' => "o",
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Ő' | 'Õ' | 'Ø' => "o",
        'ú' | 'ù' | 'û' | 'ü' | 'ű' | 'ů' => "u",
        'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ű' | 'Ů' => "u",
        'ý' | 'ÿ' | 'Ý' => "y",
        'č' | 'ć' | 'ç' | 'Č' | 'Ć' | 'Ç' => "c",
        'š' | 'ś' | 'ş' | 'Š' | 'Ś' | 'Ş' => "s",
        'ž' | 'ź' | 'ż' | 'Ž' | 'Ź' | 'Ż' => "z",
        'ř' | 'Ř' => "r",
        'ň' | 'ń' | 'ñ' | 'Ň' | 'Ń' | 'Ñ' => "n",
        'ť' | 'Ť' | 'ţ' => "t",
        'ď' | 'Ď' => "d",
        'ľ' | 'ĺ' | 'ł' | 'Ľ' | 'Ĺ' | 'Ł' => "l",
        'ß' => "ss",
        '.' | ',' | ';' | ':' | '-' | '_' | '/' | '(' | ')' | '"' | '\'' => " ",
        _ => return ch.to_string(),
    };
    s.to_string()
}

/// Formats a name in Hungarian order: "dr. Kovács János".
pub fn full_name(prefix: &str, family: &str, given: &str) -> String {
    [prefix.trim(), family.trim(), given.trim()].iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" ")
}

/// Sort name without prefix: "Kovács János" (FR-ELH-08 – prefixes do not affect order).
pub fn sort_name(family: &str, given: &str) -> String {
    full_name("", family, given)
}

// ---------------------------------------------------------------------------
// Hungarian collation
// ---------------------------------------------------------------------------

/// Collation element: (primary weight, secondary weight).
/// Primary distinguishes letters of the Hungarian alphabet; secondary
/// distinguishes short/long vowels (a/á, o/ó, ö/ő, ...), which are equal on
/// the primary level per the Hungarian spelling rules.
type Elem = (u32, u8);

// Primary weights; gaps leave room for digraphs.
const fn w(n: u32) -> u32 {
    1000 + n * 10
}

fn letter_weight(c: char) -> Option<Elem> {
    Some(match c {
        'a' => (w(1), 0),
        'á' => (w(1), 1),
        'b' => (w(2), 0),
        'c' => (w(3), 0),
        'd' => (w(5), 0),
        'e' => (w(8), 0),
        'é' => (w(8), 1),
        'f' => (w(9), 0),
        'g' => (w(10), 0),
        'h' => (w(12), 0),
        'i' => (w(13), 0),
        'í' => (w(13), 1),
        'j' => (w(14), 0),
        'k' => (w(15), 0),
        'l' => (w(16), 0),
        'm' => (w(18), 0),
        'n' => (w(19), 0),
        'o' => (w(21), 0),
        'ó' => (w(21), 1),
        'ö' => (w(22), 0),
        'ő' => (w(22), 1),
        'p' => (w(23), 0),
        'q' => (w(24), 0),
        'r' => (w(25), 0),
        's' => (w(26), 0),
        't' => (w(28), 0),
        'u' => (w(30), 0),
        'ú' => (w(30), 1),
        'ü' => (w(31), 0),
        'ű' => (w(31), 1),
        'v' => (w(32), 0),
        'w' => (w(33), 0),
        'x' => (w(34), 0),
        'y' => (w(35), 0),
        'z' => (w(36), 0),
        _ => return None,
    })
}

/// Digraph/trigraph weights. Ordered longest first.
const MULTI: &[(&str, u32)] =
    &[("dzs", w(7)), ("cs", w(4)), ("dz", w(6)), ("gy", w(11)), ("ly", w(17)), ("ny", w(20)), ("sz", w(27)), ("ty", w(29)), ("zs", w(37))];

/// Doubled long forms: "ssz" = "sz"+"sz", "ccs" = "cs"+"cs", "ddzs" = "dzs"+"dzs" ...
const DOUBLED: &[(&str, &str)] =
    &[("ddzs", "dzs"), ("ccs", "cs"), ("ddz", "dz"), ("ggy", "gy"), ("lly", "ly"), ("nny", "ny"), ("ssz", "sz"), ("tty", "ty"), ("zzs", "zs")];

fn weight_of_multi(m: &str) -> u32 {
    MULTI.iter().find(|(s, _)| *s == m).map(|(_, wt)| *wt).unwrap_or(0)
}

fn elements(s: &str) -> Vec<Elem> {
    let lower: Vec<char> = s.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut out = Vec::with_capacity(lower.len());
    let mut i = 0;
    'outer: while i < lower.len() {
        // doubled digraphs
        for (pat, base) in DOUBLED {
            let pc: Vec<char> = pat.chars().collect();
            if lower[i..].starts_with(&pc) {
                let wt = weight_of_multi(base);
                out.push((wt, 0));
                out.push((wt, 0));
                i += pc.len();
                continue 'outer;
            }
        }
        for (pat, wt) in MULTI {
            let pc: Vec<char> = pat.chars().collect();
            if lower[i..].starts_with(&pc) {
                out.push((*wt, 0));
                i += pc.len();
                continue 'outer;
            }
        }
        let c = lower[i];
        if let Some(e) = letter_weight(c) {
            out.push(e);
        } else if c.is_ascii_digit() {
            out.push((100 + c as u32 - '0' as u32, 0));
        } else if c.is_whitespace() {
            out.push((10, 0));
        } else if c.is_alphabetic() {
            // foreign letters: fold, then weigh
            let f = normalize(&c.to_string());
            match f.chars().next().and_then(letter_weight) {
                Some((p, _)) => out.push((p, 2)),
                None => out.push((5000 + c as u32, 0)),
            }
        } // punctuation ignored
        i += 1;
    }
    out
}

/// Compares two strings according to the Hungarian alphabet.
pub fn hu_compare(a: &str, b: &str) -> Ordering {
    let ea = elements(a);
    let eb = elements(b);
    let pa: Vec<u32> = ea.iter().map(|e| e.0).collect();
    let pb: Vec<u32> = eb.iter().map(|e| e.0).collect();
    pa.cmp(&pb)
        .then_with(|| {
            let sa: Vec<u8> = ea.iter().map(|e| e.1).collect();
            let sb: Vec<u8> = eb.iter().map(|e| e.1).collect();
            sa.cmp(&sb)
        })
        .then_with(|| a.cmp(b))
}

/// Binary-sortable key equivalent to [`hu_compare`]: primary weights as
/// fixed-width hex, a zero separator, then secondary weights. Stored in the
/// database so ORDER BY uses a plain index instead of a callback collation.
pub fn sort_key(s: &str) -> String {
    let e = elements(s);
    let mut out = String::with_capacity(e.len() * 5 + 8);
    for (p, _) in &e {
        out.push_str(&format!("{:04x}", p.min(&0xffff)));
    }
    out.push_str("0000");
    for (_, sec) in &e {
        out.push(char::from(b'0' + sec));
    }
    out
}

/// 1234567 → "1 234 567" (non-breaking spaces, Hungarian grouping).
pub fn thousands(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| hu_compare(a, b));
        v
    }

    #[test]
    fn normalize_removes_accents_and_case() {
        assert_eq!(normalize("Kovács  Józsefné"), "kovacs jozsefne");
        assert_eq!(normalize("ŐRSI Űrhajós"), "orsi urhajos");
        assert_eq!(normalize("dr. Szabó-Nagy"), "dr szabo nagy");
    }

    #[test]
    fn digraphs_sort_after_single_letters() {
        assert_eq!(sorted(vec!["Csonka", "Czakó", "Cukor"]), vec!["Cukor", "Czakó", "Csonka"]);
        assert_eq!(sorted(vec!["Szabó", "Sütő", "Sz"]), vec!["Sütő", "Sz", "Szabó"]);
        assert_eq!(sorted(vec!["Gyuri", "Gulyás", "Győző"]), vec!["Gulyás", "Győző", "Gyuri"]);
        assert_eq!(sorted(vec!["Zsolt", "Zoltán", "Zuzana"]), vec!["Zoltán", "Zuzana", "Zsolt"]);
        assert_eq!(sorted(vec!["Nyíri", "Nagy", "Németh"]), vec!["Nagy", "Németh", "Nyíri"]);
        assert_eq!(sorted(vec!["Lyka", "Lukács"]), vec!["Lukács", "Lyka"]);
        assert_eq!(sorted(vec!["Tyukodi", "Tóth"]), vec!["Tóth", "Tyukodi"]);
    }

    #[test]
    fn dz_and_dzs() {
        assert_eq!(sorted(vec!["Dzsida", "Dzurik", "Dudás"]), vec!["Dudás", "Dzurik", "Dzsida"]);
    }

    #[test]
    fn long_vowels_equal_on_primary_level() {
        // "Ács" vs "Adorján": á == a on primary, so c(s) vs d decides
        assert_eq!(sorted(vec!["Adorján", "Ács"]), vec!["Ács", "Adorján"]);
        // o/ó equal; ö/ő after o
        assert_eq!(sorted(vec!["Örsi", "Oláh", "Ónodi"]), vec!["Oláh", "Ónodi", "Örsi"]);
        // tie broken by secondary: short before long
        assert_eq!(hu_compare("Kerek", "Kerék"), Ordering::Less);
    }

    #[test]
    fn doubled_digraphs() {
        // "Hosszú" = h o sz sz ú ; must sort after "Hoszu"? h o sz u  vs h o sz sz u -> sz < u? no: compare 3rd elem: u(30) vs sz(27)
        assert_eq!(sorted(vec!["Hoszu", "Hosszú"]), vec!["Hosszú", "Hoszu"]);
        assert_eq!(elements("ssz").len(), 2);
    }

    #[test]
    fn sort_key_matches_collation() {
        let names = [
            "Csonka", "Czakó", "Cukor", "Szabó", "Sütő", "Gyuri", "Gulyás", "Győző", "Ács", "Adorján", "Kerek", "Kerék", "Kis", "Kis Anna", "Kiss", "Hosszú",
            "Hoszu", "Zsolt", "Zoltán", "Dzsida", "Dzurik", "Ödön", "Oláh",
        ];
        let mut a: Vec<&str> = names.to_vec();
        let mut b: Vec<&str> = names.to_vec();
        a.sort_by(|x, y| hu_compare(x, y));
        b.sort_by_key(|x| sort_key(x));
        assert_eq!(a, b);
    }

    #[test]
    fn full_name_order() {
        assert_eq!(full_name("dr.", "Kovács", "János"), "dr. Kovács János");
        assert_eq!(full_name("", "Kovács", "János"), "Kovács János");
    }
}
