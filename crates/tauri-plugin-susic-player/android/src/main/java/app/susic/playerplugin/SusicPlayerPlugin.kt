package app.susic.playerplugin

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.OpenableColumns
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class LoadArgs {
    var path: String = ""
    var title: String = ""
    var artist: String = ""
    var album: String = ""
    var artwork: String? = null
    var startMs: Long = 0
    var autoplay: Boolean = true
}

@InvokeArg class SeekArgs { var positionMs: Long = 0 }
@InvokeArg class VolumeArgs { var volume: Float = 1f }
@InvokeArg class PickArgs { var dest: String = "" }

@TauriPlugin
class SusicPlayerPlugin(private val activity: Activity) : Plugin(activity) {
    private var pickDest: String = ""

    override fun load(webView: WebView) {
        super.load(webView)
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 7)
        }
        PlayerHub.run(activity) { /* create player */ }
        activity.startService(Intent(activity, PlaybackService::class.java))
    }

    @Command
    fun load(invoke: Invoke) {
        val a = invoke.parseArgs(LoadArgs::class.java)
        PlayerHub.load(activity, a.path, a.title, a.artist, a.album, a.artwork, a.startMs, a.autoplay)
        invoke.resolve()
    }

    @Command fun play(invoke: Invoke) { PlayerHub.run(activity) { it.play() }; invoke.resolve() }
    @Command fun pause(invoke: Invoke) { PlayerHub.run(activity) { it.pause() }; invoke.resolve() }
    @Command fun stop(invoke: Invoke) { PlayerHub.run(activity) { it.stop(); it.clearMediaItems() }; PlayerHub.loaded = false; invoke.resolve() }

    @Command
    fun seek(invoke: Invoke) {
        val a = invoke.parseArgs(SeekArgs::class.java)
        PlayerHub.run(activity) { it.seekTo(a.positionMs) }
        invoke.resolve()
    }

    @Command
    fun setVolume(invoke: Invoke) {
        val a = invoke.parseArgs(VolumeArgs::class.java)
        PlayerHub.run(activity) { it.volume = a.volume }
        invoke.resolve()
    }

    /** Snapshot + drained events (ended / next / prev / error:...). */
    @Command
    fun state(invoke: Invoke) {
        val o = JSObject()
        o.put("playing", PlayerHub.playing)
        o.put("positionMs", PlayerHub.positionMs)
        o.put("durationMs", PlayerHub.durationMs)
        o.put("loaded", PlayerHub.loaded)
        val ev = JSArray()
        while (true) { ev.put(PlayerHub.events.poll() ?: break) }
        o.put("events", ev)
        invoke.resolve(o)
    }

    @Command
    fun pickAudio(invoke: Invoke) {
        val a = invoke.parseArgs(PickArgs::class.java)
        pickDest = a.dest
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "audio/*"
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("audio/*", "application/ogg"))
        }
        startActivityForResult(invoke, intent, "pickResult")
    }

    @ActivityCallback
    private fun pickResult(invoke: Invoke, result: ActivityResult) {
        val out = JSObject()
        val paths = JSArray()
        val data = result.data
        if (result.resultCode != Activity.RESULT_OK || data == null) {
            out.put("paths", paths); invoke.resolve(out); return
        }
        val uris = mutableListOf<android.net.Uri>()
        data.data?.let { uris.add(it) }
        data.clipData?.let { cd -> for (i in 0 until cd.itemCount) uris.add(cd.getItemAt(i).uri) }
        // Copy off the main thread into app-private storage so playback never
        // depends on external permissions or the picker's URI grants.
        Thread {
            val dir = File(pickDest).apply { mkdirs() }
            for (u in uris) {
                try {
                    var name = "audio-${System.nanoTime()}"
                    var size = -1L
                    activity.contentResolver.query(u, null, null, null, null)?.use { c ->
                        if (c.moveToFirst()) {
                            val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                            val s = c.getColumnIndex(OpenableColumns.SIZE)
                            if (n >= 0) name = c.getString(n)
                            if (s >= 0) size = c.getLong(s)
                        }
                    }
                    name = name.replace(Regex("[/\\\\:*?\"<>|]"), "_")
                    var dest = File(dir, name)
                    if (dest.exists() && (size < 0 || dest.length() == size)) { paths.put(dest.absolutePath); continue }
                    var i = 1
                    while (dest.exists()) { dest = File(dir, "${dest.nameWithoutExtension.removeSuffix("-${i - 1}")}-$i.${dest.extension}"); i++ }
                    val tmp = File(dir, dest.name + ".part")
                    activity.contentResolver.openInputStream(u)?.use { input ->
                        tmp.outputStream().use { input.copyTo(it) }
                    }
                    tmp.renameTo(dest)
                    paths.put(dest.absolutePath)
                } catch (e: Exception) { /* skip unreadable file */ }
            }
            out.put("paths", paths)
            invoke.resolve(out)
        }.start()
    }
}
