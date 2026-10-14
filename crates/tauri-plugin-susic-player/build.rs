const COMMANDS: &[&str] = &[
    "load",
    "play",
    "pause",
    "seek",
    "set_volume",
    "stop",
    "state",
    "pick_audio",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
