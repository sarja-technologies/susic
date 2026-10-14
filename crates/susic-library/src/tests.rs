use super::*;

fn nt(title: &str, artist: &str, album: &str, path: &str, hash: &str) -> NewTrack {
    NewTrack {
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        path: path.into(),
        hash: hash.into(),
        duration_ms: 1000,
        size_bytes: 10,
        source: "test".into(),
        ..Default::default()
    }
}

fn lib() -> Library {
    let l = Library::open_memory().unwrap();
    for (i, (t, a, al)) in [
        ("Alpha", "Ann", "One"),
        ("Beta", "Ann", "One"),
        ("Gamma", "Bob", "Two"),
    ]
    .iter()
    .enumerate()
    {
        l.upsert_track(&nt(t, a, al, &format!("/m/{t}.mp3"), &format!("h{i}")))
            .unwrap();
    }
    l
}

#[test]
fn migrations_reach_latest() {
    let l = Library::open_memory().unwrap();
    assert_eq!(l.schema_version().unwrap() as usize, migrations::LATEST);
}

#[test]
fn migrations_preserve_existing_data() {
    let dir = std::env::temp_dir().join(format!("susic-mig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lib.db");
    // Simulate a library created by schema v1 only.
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(migrations_v1()).unwrap();
        c.pragma_update(None, "user_version", 1).unwrap();
        c.execute(
            "INSERT INTO tracks (title, path, added_at) VALUES ('Old', '/old.mp3', 1)",
            [],
        )
        .unwrap();
    }
    let l = Library::open(&path).unwrap();
    assert_eq!(l.schema_version().unwrap() as usize, migrations::LATEST);
    let all = l.list_tracks(&TrackQuery::default()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "Old");
    assert!(l.import_jobs().unwrap().is_empty());
    std::fs::remove_dir_all(dir).ok();
}

fn migrations_v1() -> &'static str {
    // First entry of the migration list is applied verbatim.
    include_str!("migrations.rs")
        .split("\"CREATE TABLE tracks")
        .nth(1)
        .map(|s| {
            let body = s.split("\",\n").next().unwrap();
            Box::leak(format!("CREATE TABLE tracks{body}").into_boxed_str()) as &'static str
        })
        .unwrap()
}

#[test]
fn dedupes_by_path_and_hash() {
    let l = lib();
    assert!(matches!(
        l.upsert_track(&nt("Alpha", "Ann", "One", "/m/Alpha.mp3", "h0"))
            .unwrap(),
        Upsert::Updated(_)
    ));
    assert!(matches!(
        l.upsert_track(&nt("Alpha", "Ann", "One", "/copy/Alpha.mp3", "h0"))
            .unwrap(),
        Upsert::Duplicate(_)
    ));
    assert_eq!(l.stats().unwrap().tracks, 3);
}

#[test]
fn search_and_sort() {
    let l = lib();
    let r = l
        .list_tracks(&TrackQuery {
            search: Some("ann one".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(r.len(), 2);
    let r = l
        .list_tracks(&TrackQuery {
            sort: Some("title".into()),
            desc: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(r[0].title, "Gamma");
    let r = l
        .list_tracks(&TrackQuery {
            search: Some("100%".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(r.is_empty());
}

#[test]
fn albums_artists() {
    let l = lib();
    let al = l.albums(None).unwrap();
    assert_eq!(al.len(), 2);
    assert_eq!(al[0].album, "One");
    assert_eq!(al[0].track_count, 2);
    assert_eq!(l.artists(None).unwrap().len(), 2);
}

#[test]
fn favourites_and_history() {
    let l = lib();
    let id = l.list_tracks(&TrackQuery::default()).unwrap()[0].id;
    l.set_favourite(id, true).unwrap();
    l.mark_played(id).unwrap();
    l.mark_played(id).unwrap();
    let t = l.get_track(id).unwrap().unwrap();
    assert!(t.favourite);
    assert_eq!(t.play_count, 2);
    assert_eq!(l.recently_played(10).unwrap().len(), 1);
    assert_eq!(l.most_played(10).unwrap()[0].id, id);
    assert_eq!(
        l.list_tracks(&TrackQuery {
            favourites_only: Some(true),
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn edits_are_separate_from_original() {
    let l = lib();
    let id = l.list_tracks(&TrackQuery::default()).unwrap()[0].id;
    l.edit_track(
        id,
        &TrackEdit {
            title: Some("Edited".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(l.get_track(id).unwrap().unwrap().title, "Edited");
    l.clear_edit(id).unwrap();
    assert_eq!(l.get_track(id).unwrap().unwrap().title, "Alpha");
}

#[test]
fn playlists_roundtrip() {
    let l = lib();
    let ids: Vec<i64> = l
        .list_tracks(&TrackQuery::default())
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    let p = l.create_playlist("Mix").unwrap();
    l.add_to_playlist(p, &ids).unwrap();
    l.add_to_playlist(p, &[ids[0]]).unwrap(); // duplicates allowed
    assert_eq!(l.playlist_track_ids(p).unwrap().len(), 4);
    l.reorder_playlist(p, 0, 2).unwrap();
    assert_eq!(l.playlist_track_ids(p).unwrap()[2], ids[0]);
    l.remove_from_playlist(p, 3).unwrap();
    assert_eq!(l.playlist_track_ids(p).unwrap().len(), 3);
    let d = l.duplicate_playlist(p).unwrap();
    assert_eq!(l.playlist_track_ids(d).unwrap().len(), 3);
    l.rename_playlist(p, "Renamed").unwrap();
    assert!(l.playlists().unwrap().iter().any(|x| x.name == "Renamed"));
    l.delete_playlist(p).unwrap();
    assert_eq!(l.playlists().unwrap().len(), 1);
    // Deleting a track must not break remaining playlists.
    l.remove_track(ids[0]).unwrap();
    assert_eq!(l.playlist_track_ids(d).unwrap().len(), 2);
}

#[test]
fn settings_persist() {
    let l = lib();
    assert_eq!(l.get_setting("k").unwrap(), None);
    l.set_setting("k", "v1").unwrap();
    l.set_setting("k", "v2").unwrap();
    assert_eq!(l.get_setting("k").unwrap().as_deref(), Some("v2"));
}

#[test]
fn interrupted_jobs_flagged() {
    let l = lib();
    let j = l.add_import_job("https://example.com/a").unwrap();
    l.update_import_job(j, "running", 0.3, "").unwrap();
    assert_eq!(l.mark_interrupted_jobs().unwrap(), 1);
    assert_eq!(l.import_jobs().unwrap()[0].status, "interrupted");
}
