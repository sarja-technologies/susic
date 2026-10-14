use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricLine {
    /// Milliseconds; `None` for unsynchronised lyrics.
    pub time_ms: Option<i64>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Lyrics {
    pub synced: bool,
    pub lines: Vec<LyricLine>,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Parse LRC (synchronised) or plain text. Supports multiple timestamps per
    /// line, `[offset:+/-ms]`, and ignores metadata tags like `[ar:...]`.
    pub fn parse(src: &str) -> Lyrics {
        let mut lines = Vec::new();
        let mut offset = 0i64;
        let mut any_timed = false;
        let mut plain = Vec::new();
        for raw in src.lines() {
            let raw = raw.trim_end_matches('\r').trim();
            if raw.is_empty() {
                continue;
            }
            let mut rest = raw;
            let mut stamps = Vec::new();
            while let Some(stripped) = rest.strip_prefix('[') {
                let Some(end) = stripped.find(']') else { break };
                let tag = &stripped[..end];
                if let Some(t) = parse_stamp(tag) {
                    stamps.push(t);
                } else if let Some(v) = tag.strip_prefix("offset:") {
                    offset = v.trim().parse().unwrap_or(0);
                } else if !tag.contains(':') {
                    break;
                }
                rest = stripped[end + 1..].trim_start();
                if stamps.is_empty() && !tag.contains(':') {
                    break;
                }
            }
            if stamps.is_empty() {
                // Metadata tag lines like [ti:..] leave `rest` empty.
                if raw.starts_with('[') && rest.is_empty() {
                    continue;
                }
                plain.push(raw.to_string());
            } else {
                any_timed = true;
                for t in stamps {
                    lines.push(LyricLine {
                        time_ms: Some(t - offset),
                        text: rest.to_string(),
                    });
                }
            }
        }
        if any_timed {
            lines.sort_by_key(|l| l.time_ms);
            Lyrics {
                synced: true,
                lines,
            }
        } else {
            Lyrics {
                synced: false,
                lines: plain
                    .into_iter()
                    .map(|text| LyricLine {
                        time_ms: None,
                        text,
                    })
                    .collect(),
            }
        }
    }

    /// Index of the active line at `pos_ms` (+ user `offset_ms`), if any.
    pub fn active_index(&self, pos_ms: i64, offset_ms: i64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let t = pos_ms + offset_ms;
        let idx = self.lines.partition_point(|l| l.time_ms.unwrap_or(0) <= t);
        idx.checked_sub(1)
    }
}

fn parse_stamp(tag: &str) -> Option<i64> {
    let (m, rest) = tag.split_once(':')?;
    let m: i64 = m.trim().parse().ok()?;
    let (s, frac) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let s: i64 = s.trim().parse().ok()?;
    let ms = match frac {
        None => 0,
        Some(f) => {
            let digits: String = f.chars().take(3).collect();
            let v: i64 = digits.parse().ok()?;
            match digits.len() {
                1 => v * 100,
                2 => v * 10,
                _ => v,
            }
        }
    };
    Some((m * 60 + s) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced() {
        let l = Lyrics::parse("[ar:X]\n[00:01.50]Hello\n[00:03.00][00:10.00]World\n");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].time_ms, Some(1500));
        assert_eq!(l.lines[0].text, "Hello");
        assert_eq!(l.active_index(2000, 0), Some(0));
        assert_eq!(l.active_index(500, 0), None);
        assert_eq!(l.active_index(500, 1000), Some(0));
        assert_eq!(l.active_index(11_000, 0), Some(2));
    }

    #[test]
    fn parses_plain() {
        let l = Lyrics::parse("one\ntwo\n\nthree");
        assert!(!l.synced);
        assert_eq!(l.lines.len(), 3);
    }

    #[test]
    fn applies_offset() {
        let l = Lyrics::parse("[offset:500]\n[00:02.00]x");
        assert_eq!(l.lines[0].time_ms, Some(1500));
    }
}
