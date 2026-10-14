package app.susic.playerplugin

import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/** Foreground media service: notification, lock-screen and Bluetooth controls. */
class PlaybackService : MediaSessionService() {
    private var session: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        session = MediaSession.Builder(this, PlayerHub.sessionPlayer(this)).build()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onTaskRemoved(rootIntent: android.content.Intent?) {
        // Keep playing when the UI is swiped away; stop only if paused.
        val p = PlayerHub.player(this)
        if (!p.playWhenReady || p.mediaItemCount == 0) stopSelf()
    }

    override fun onDestroy() {
        session?.release()
        session = null
        super.onDestroy()
    }
}
