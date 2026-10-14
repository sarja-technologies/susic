//! Bridge to the Kotlin Media3 player on Android. On other platforms this
//! plugin is an inert no-op so the workspace builds everywhere.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "android")]
pub struct SusicPlayer<R: Runtime>(pub tauri::plugin::PluginHandle<R>);
#[cfg(not(target_os = "android"))]
pub struct SusicPlayer<R: Runtime>(std::marker::PhantomData<fn() -> R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("susic-player")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("app.susic.playerplugin", "SusicPlayerPlugin")?;
                app.manage(SusicPlayer(handle));
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = api;
                app.manage(SusicPlayer::<R>(std::marker::PhantomData));
            }
            Ok(())
        })
        .build()
}
