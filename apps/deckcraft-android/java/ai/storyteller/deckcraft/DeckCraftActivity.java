package ai.storyteller.deckcraft;

import android.app.NativeActivity;
import android.view.KeyEvent;

/**
 * DeckCraft's Android activity: Back dismisses, it does not leave the presentation.
 *
 * android.app.NativeActivity overrides no key method and no back method, and from Android 13 the
 * window's own back dispatcher decides Back before any key event reaches the app, so the dispatcher
 * finishes the activity and DeckCraft goes to the background with a dialog still open. Registering a
 * back callback is the way this window answers Back, so this activity registers one (Android 13+) and
 * answers the legacy Back key (Android 12 and below) in onKeyDown/onKeyUp/onBackPressed.
 *
 * The answer is DeckCraft's Escape, which dismisses a dialog, a menu or a cancellation on every host.
 * Java cannot hand a key event to NativeActivity's native input queue, so the answer crosses the
 * boundary as a record rather than a key: the callback calls the activity's own native library
 * (`backInvoked`, bound to this class at start-up by JNI in android_access.rs, which only notes the
 * gesture); DeckCraft's event loop takes that record and answers Escape itself, on the loop thread.
 * Home still leaves the app.
 *
 * An unbound `backInvoked` is caught rather than left to throw: an exception from this callback
 * reaches Android's message queue and Android kills the process, which would lose the presentation —
 * worse than a Back that does nothing, and the dialog's own close control still dismisses it.
 *
 * This is platform glue, not app logic: no presentation decision is made here, no DeckCraft API is
 * called, and nothing finishes the activity. See AGENTS.md §3 and §4 for the named exceptions, and
 * `cargo xtask apk` for how this class lands in the APK's classes.dex.
 */
public class DeckCraftActivity extends NativeActivity {
    /** Back, recorded for DeckCraft's loop by the activity's native library. */
    private static native void backInvoked();

    /** Android 13 and newer answer Back themselves through the window's back dispatcher, and
     * `NativeActivity` registers no callback, so the dispatcher finishes the activity. Registering
     * one is the public way for a window to answer Back. */
    @Override
    protected void onCreate(android.os.Bundle saved) {
        super.onCreate(saved);
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, this::answerBack);
        }
    }

    /** The gesture Back: record it, don't leave. */
    private void answerBack() {
        try {
            backInvoked();
        } catch (Throwable unbound) {
            android.util.Log.e("DeckCraft", "Back could not be recorded for DeckCraft's loop", unbound);
        }
    }

    /** The legacy Back key (Android 12 and below, where Back arrives as a key event). */
    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            answerBack();
            return true;
        }
        return super.onKeyDown(keyCode, event);
    }

    @Override
    public boolean onKeyUp(int keyCode, KeyEvent event) {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            answerBack();
            return true;
        }
        return super.onKeyUp(keyCode, event);
    }

    /** The legacy back request, on the same old devices as the key above. */
    @Override
    public void onBackPressed() {
        answerBack();
    }
}
