package app.susic.playerplugin

import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/** Foreground media service: notification, lock-screen and Bluetooth controls. */
class PlaybackService : MediaSessionService() {
    private var session: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        val s = MediaSession.Builder(this, PlayerHub.sessionPlayer(this)).build()
        session = s
        // Register with the notification manager even when no MediaController is bound,
        // otherwise no notification (and no foreground state) is ever posted.
        addSession(s)
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
