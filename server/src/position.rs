//! Fractional-index ordering keys.
//!
//! A key is a non-empty string of base-62 digits, read as a fraction in (0, 1) and compared
//! lexicographically (byte order matches digit order). There is always room for a key
//! between any two keys, so moving an item only rewrites that item's row.
//!
//! Keys never end in '0', which keeps every key strictly between its neighbours.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: usize = DIGITS.len();

fn digit(c: u8) -> usize {
    DIGITS
        .iter()
        .position(|&d| d == c)
        .expect("position key contains a non base-62 character")
}

/// A key strictly between `a` and `b`. `None` means the start or the end of the list.
pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    let a = a.unwrap_or("");
    if let Some(b) = b {
        assert!(a < b, "position keys out of order: {a:?} >= {b:?}");
    }
    midpoint(a.as_bytes(), b.map(str::as_bytes))
}

/// A key after `last` (or the first key of an empty list). Appending is the common case,
/// so it bumps the last digit that has room instead of halving, which keeps keys short.
pub fn after(last: Option<&str>) -> String {
    let Some(last) = last else {
        return between(None, None);
    };
    let bytes = last.as_bytes();
    match bytes.iter().rposition(|&c| digit(c) < BASE - 1) {
        Some(i) => {
            let mut out = bytes[..i].to_vec();
            out.push(DIGITS[digit(bytes[i]) + 1]);
            String::from_utf8(out).expect("ascii")
        }
        // Every digit is already 'z': extend instead.
        None => format!("{last}{}", DIGITS[BASE / 2] as char),
    }
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Strip the common prefix (treating a missing digit in `a` as '0').
        let n = (0..b.len())
            .take_while(|&i| a.get(i).copied().unwrap_or(b'0') == b[i])
            .count();
        if n > 0 {
            let rest = midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..]));
            return format!("{}{rest}", std::str::from_utf8(&b[..n]).expect("ascii"));
        }
    }

    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.and_then(|b| b.first()).map_or(BASE, |&c| digit(c));
    if db - da > 1 {
        return (DIGITS[(da + db).div_ceil(2)] as char).to_string();
    }
    // Leading digits are adjacent.
    match b {
        Some(b) if b.len() > 1 => (b[0] as char).to_string(),
        _ => format!(
            "{}{}",
            DIGITS[da] as char,
            midpoint(a.get(1..).unwrap_or(&[]), None)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_key_is_middle() {
        assert_eq!(between(None, None), "V");
    }

    #[test]
    fn between_is_strictly_ordered() {
        let cases = [
            (None, None),
            (Some("V"), None),
            (None, Some("V")),
            (Some("V"), Some("W")),
            (Some("V"), Some("V1")),
            (Some("z"), None),
            (None, Some("01")),
            (Some("a0V"), Some("a1")),
        ];
        for (a, b) in cases {
            let k = between(a, b);
            assert!(!k.ends_with('0'), "{k}");
            if let Some(a) = a {
                assert!(a < k.as_str(), "{a} < {k}");
            }
            if let Some(b) = b {
                assert!(k.as_str() < b, "{k} < {b}");
            }
        }
    }

    #[test]
    fn repeated_inserts_stay_sorted() {
        // Insert always at the front, the back and the middle; the list must stay sorted.
        let mut keys = vec![between(None, None)];
        for i in 0..300 {
            let k = match i % 3 {
                0 => between(None, Some(&keys[0])),
                1 => after(keys.last().map(String::as_str)),
                _ => {
                    let m = keys.len() / 2;
                    between(Some(&keys[m - 1]), Some(&keys[m]))
                }
            };
            keys.push(k);
            keys.sort();
            assert!(keys.windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn appends_grow_slowly() {
        let mut last: Option<String> = None;
        for _ in 0..1000 {
            let k = after(last.as_deref());
            if let Some(l) = &last {
                assert!(l < &k);
            }
            last = Some(k);
        }
        assert!(last.unwrap().len() <= 40);
    }
}
