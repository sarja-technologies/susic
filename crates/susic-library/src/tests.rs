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

#[test]
fn server_tracks_link_to_local_copies_instead_of_duplicating() {
    let l = lib();
    // Same recording as local "Alpha" (hash h0): linked, no new row.
    let before = l.stats().unwrap().tracks;
    let id = l.upsert_remote(&nt("Alpha", "Ann", "One", "", "h0"), 7).unwrap();
    assert_eq!(l.stats().unwrap().tracks, before);
    let t = l.get_track(id).unwrap().unwrap();
    assert_eq!(t.availability, "local+server");
    assert_eq!(t.remote_id, Some(7));
    // Unknown recording becomes a server-only row.
    let sid = l.upsert_remote(&nt("Delta", "Cy", "Three", "", "h9"), 8).unwrap();
    let s = l.get_track(sid).unwrap().unwrap();
    assert_eq!((s.availability.as_str(), s.source.as_str()), ("server", "server"));
    // Re-sync is idempotent.
    assert_eq!(l.upsert_remote(&nt("Delta", "Cy", "Three", "", "h9"), 8).unwrap(), sid);
    // Server-only rows are never treated as missing local files.
    assert!(l.track_paths().unwrap().iter().all(|(i, _)| *i != sid));
    // Favourite + offline copy survive; availability reflects the cache.
    l.set_favourite(sid, true).unwrap();
    l.set_cache(sid, Some("/cache/8.flac"), true).unwrap();
    assert_eq!(l.get_track(sid).unwrap().unwrap().availability, "downloaded");
    // Importing the same recording locally adopts the local file into this row.
    let r = l.upsert_track(&nt("Delta", "Cy", "Three", "/m/Delta.mp3", "h9")).unwrap();
    assert_eq!(r, Upsert::Updated(sid));
    let t = l.get_track(sid).unwrap().unwrap();
    assert!(t.favourite);
    assert_eq!(t.path, "/m/Delta.mp3");
    assert_eq!(t.availability, "local+server");
}

#[test]
fn removed_remote_tracks_keep_offline_copies() {
    let l = Library::open_memory().unwrap();
    let a = l.upsert_remote(&nt("A", "x", "y", "", "ha"), 1).unwrap();
    let b = l.upsert_remote(&nt("B", "x", "y", "", "hb"), 2).unwrap();
    l.set_cache(b, Some("/cache/b"), false).unwrap();
    assert_eq!(l.remote_removed(&[1, 2]).unwrap(), 1);
    assert!(l.get_track(a).unwrap().is_none());
    let kept = l.get_track(b).unwrap().unwrap();
    assert_eq!((kept.remote_id, kept.availability.as_str()), (None, "downloaded"));
}

#[test]
fn playlist_deletion_leaves_tombstone_only_when_synced() {
    let l = Library::open_memory().unwrap();
    let p1 = l.create_playlist("plain").unwrap();
    let p2 = l.create_playlist("synced").unwrap();
    l.set_playlist_remote_uid(p2, "uid-2").unwrap();
    l.delete_playlist(p1).unwrap();
    assert!(l.take_playlist_tombstones().unwrap().is_empty());
    l.delete_playlist(p2).unwrap();
    assert_eq!(l.take_playlist_tombstones().unwrap()[0].0, "uid-2");
}

/// Scale check (run with `cargo test -p susic-library --release -- --ignored --nocapture`).
#[test]
#[ignore]
fn bench_large_library() {
    let l = Library::open_memory().unwrap();
    let t0 = std::time::Instant::now();
    for i in 0..50_000 {
        l.upsert_track(&nt(
            &format!("Song number {i}"),
            &format!("Artist {}", i % 900),
            &format!("Album {}", i % 4000),
            &format!("/m/{i}.mp3"),
            &format!("h{i}"),
        ))
        .unwrap();
    }
    println!("insert 50k: {:?}", t0.elapsed());
    let t = std::time::Instant::now();
    let r = l
        .list_tracks(&TrackQuery { search: Some("number 4999".into()), ..Default::default() })
        .unwrap();
    println!("search ({} hits): {:?}", r.len(), t.elapsed());
    let t = std::time::Instant::now();
    let all = l.list_tracks(&TrackQuery::default()).unwrap();
    println!("list all {}: {:?}", all.len(), t.elapsed());
    let t = std::time::Instant::now();
    let a = l.albums(None).unwrap();
    println!("albums {}: {:?}", a.len(), t.elapsed());
}
