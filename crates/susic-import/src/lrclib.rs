//! Optional LRCLIB (https://lrclib.net) lookup. Only invoked on explicit user
//! request; results are cached locally by the caller.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Resp {
    #[serde(rename = "syncedLyrics")]
    synced: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain: Option<String>,
}

pub fn fetch(
    title: &str,
    artist: &str,
    album: &str,
    duration_s: i64,
) -> crate::Result<Option<String>> {
    let resp = ureq::get("https://lrclib.net/api/get")
        .query("track_name", title)
        .query("artist_name", artist)
        .query("album_name", album)
        .query("duration", &duration_s.to_string())
        .set(
            "User-Agent",
            concat!(
                "Susic/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/sarja-technologies/susic)"
            ),
        )
        .timeout(std::time::Duration::from_secs(10))
        .call();
    match resp {
        Ok(r) => {
            let body: Resp = r
                .into_json()
                .map_err(|e| crate::Error::Msg(e.to_string()))?;
            Ok(body
                .synced
                .filter(|s| !s.trim().is_empty())
                .or(body.plain.filter(|s| !s.trim().is_empty())))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(crate::Error::Msg(format!("LRCLIB request failed: {e}"))),
    }
}
