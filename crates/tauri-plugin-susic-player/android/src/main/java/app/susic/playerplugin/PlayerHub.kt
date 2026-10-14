package app.susic.playerplugin

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import java.io.File
import java.util.concurrent.ConcurrentLinkedQueue

/**
 * Process-wide owner of the ExoPlayer. Lives independently of the WebView so
 * playback continues with the UI in the background. All player access happens
 * on the main thread; the Rust side reads [snapshot] and drains [events].
 */
object PlayerHub {
    private val main = Handler(Looper.getMainLooper())
    private var exo: ExoPlayer? = null
    private var session: Player? = null
    val events = ConcurrentLinkedQueue<String>()

    @Volatile var playing = false
    @Volatile var positionMs = 0L
    @Volatile var durationMs = 0L
    @Volatile var loaded = false
    @Volatile var error: String? = null

    /** Must be called on the main thread. */
    fun player(ctx: Context): ExoPlayer {
        exo?.let { return it }
        val p = ExoPlayer.Builder(ctx.applicationContext)
            .setAudioAttributes(
                AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(),
                /* handleAudioFocus = */ true
            )
            .setHandleAudioBecomingNoisy(true)
            .setWakeMode(C.WAKE_MODE_LOCAL)
            .build()
        p.addListener(object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (state == Player.STATE_ENDED) {
                    loaded = false
                    events.add("ended")
                }
            }
            override fun onPlayerError(e: androidx.media3.common.PlaybackException) {
                loaded = false
                error = e.message ?: "Playback error"
                events.add("error:" + (e.message ?: "Playback error"))
            }
        })
        exo = p
        session = RemoteAwarePlayer(p)
        tick()
        return p
    }

    /** The player exposed to MediaSession: next/previous become events for Rust. */
    fun sessionPlayer(ctx: Context): Player {
        player(ctx)
        return session!!
    }

    private fun tick() {
        exo?.let {
            playing = it.isPlaying || (it.playWhenReady && it.playbackState == Player.STATE_BUFFERING)
            positionMs = it.currentPosition.coerceAtLeast(0)
            if (it.duration != C.TIME_UNSET) durationMs = it.duration
        }
        main.postDelayed({ tick() }, 250)
    }

    fun run(ctx: Context, block: (ExoPlayer) -> Unit) {
        main.post { block(player(ctx)) }
    }

    fun load(ctx: Context, path: String, title: String, artist: String, album: String, artwork: String?, startMs: Long, autoplay: Boolean) {
        run(ctx) { p ->
            val meta = MediaMetadata.Builder().setTitle(title).setArtist(artist).setAlbumTitle(album)
            if (!artwork.isNullOrEmpty()) meta.setArtworkUri(Uri.fromFile(File(artwork)))
            val item = MediaItem.Builder().setUri(Uri.fromFile(File(path))).setMediaMetadata(meta.build()).build()
            error = null
            p.setMediaItem(item, startMs)
            p.prepare()
            p.playWhenReady = autoplay
            loaded = true
        }
    }
}

private class RemoteAwarePlayer(p: Player) : ForwardingPlayer(p) {
    override fun getAvailableCommands(): Player.Commands =
        super.getAvailableCommands().buildUpon()
            .add(Player.COMMAND_SEEK_TO_NEXT).add(Player.COMMAND_SEEK_TO_PREVIOUS)
            .add(Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM).add(Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM)
            .build()

    override fun isCommandAvailable(command: Int) = availableCommands.contains(command)
    override fun hasNextMediaItem() = true
    override fun hasPreviousMediaItem() = true
    override fun seekToNext() { PlayerHub.events.add("next") }
    override fun seekToNextMediaItem() { PlayerHub.events.add("next") }
    override fun seekToPrevious() { PlayerHub.events.add("prev") }
    override fun seekToPreviousMediaItem() { PlayerHub.events.add("prev") }
}
