//! Android's "All files access" grant (`MANAGE_EXTERNAL_STORAGE`), reached over JNI.
//!
//! The in-app file browser (`deckcraft_ui_egui::file_browser`) would otherwise only ever see the
//! app-private directory. To let a save or export reach shared storage (`/storage/emulated/0`),
//! the user must grant "All files access" — a special permission Android never grants silently:
//! the app can only *check* whether it is held and *open the screen* where the user toggles it
//! (Android 11+), or show the ordinary runtime permission dialog (Android 9, whose legacy
//! `WRITE_EXTERNAL_STORAGE` still opens the whole shared tree). Android 10 targets scoped
//! storage, where no grant opens the shared tree, so nothing is offered there. That is all this
//! module does — it never reads or writes storage itself, so the browser's own root-clamping
//! still keeps every answer inside the root the user is browsing.
//!
//! Every JNI call can fail (no JVM, an API level without the method, a device that refuses the
//! intent) and every failure degrades to "not granted" / "nothing happened", logged at worst —
//! never a panic, which this crate's lints deny too.
//!
//! This is the entry crate's one piece of real `unsafe`: `jni`'s entry points to the activity's
//! raw JVM (`JavaVM::from_raw`, `JObject::from_raw`, and the environment's own function table) have
//! no safe form, which is why AGENTS.md (Never crash) names this file as the workspace's `unsafe`
//! exception and confines it here — the module holds the pointers, never lets them leave, and hands
//! `deckcraft::SharedStorageAccess` plain closures, so the shared app crates stay unsafe-free.
//! `docs/architecture.md` records the exception.
//!
//! The same module also records Android's Back (`backInvoked` below), which arrives over JNI from
//! DeckCraft's activity: Android's back dispatcher answers Back before the app can see a key event,
//! so the activity registers a callback and this notes it — binding that callback to this library is
//! the part JNI's name lookup cannot do on its own. That is all it does with the presentation:
//! DeckCraft's own loop answers the recorded Back as Escape (`apps/deckcraft/src/lib.rs`).

#![allow(unsafe_code)] // the named exception above: jni's raw JVM handles have no safe spelling

use jni::objects::{JClass, JObject, JValue};
use jni::{JNIEnv, JavaVM};
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

/// Back, recorded by DeckCraft's activity. Android's own back dispatcher answers Back *before* any
/// key event reaches the app (`NativeActivity` registers no callback), so the activity registers one
/// and its callback calls the static native method below. Recording is all that happens here: the
/// app's loop takes it (`take_back`) and answers Escape on its own thread, which is what dismisses a
/// dialog, a menu or a cancellation. Nothing in this path touches the presentation directly.
static BACK_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The native half of `DeckCraftActivity.backInvoked()`: recording the gesture is all it does.
/// Bound to the activity's class by [`register_back`], not found by JNI's name lookup (see there).
extern "C" fn record_back(_env: JNIEnv<'static>, _class: JClass<'static>) {
    BACK_REQUESTED.store(true, Ordering::Relaxed);
}

/// Take the recorded Back: one dismissal per Back, so repeated frames cannot dismiss twice.
pub fn take_back() -> bool {
    BACK_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Bind [`record_back`] to `DeckCraftActivity.backInvoked()` on the activity's own class.
///
/// JNI's usual name-based lookup cannot find this library's symbol: `NativeActivity` loads the
/// library from framework code, so it lands in the framework's class-loader namespace while
/// `DeckCraftActivity` comes from the app's loader, and the JVM searches only the loader that
/// defined the class. On a Tab S11 the callback then threw `UnsatisfiedLinkError`, Android reported
/// it through the activity's message queue, and the process died mid-dialog — the very crash this
/// arc is meant to remove. Registering against the class the activity is an instance of is the
/// spelling that reaches it, and it needs no exported symbol.
///
/// Called once at start-up, before any gesture can arrive. Returns whether the binding exists; the
/// activity catches the failure rather than letting an unbound call crash the presentation.
pub fn register_back(access: AndroidAccess) -> bool {
    let bound = access.with_env(|env| -> jni::errors::Result<bool> {
        let class = env.get_object_class(access.activity())?;
        // The names are static strings the JVM copies, and `record_back` lives in this library for
        // as long as the app does, so the registration stays valid until the activity is destroyed.
        let methods = [jni::sys::JNINativeMethod {
            name: b"backInvoked\0".as_ptr() as *mut c_char,
            signature: b"()V\0".as_ptr() as *mut c_char,
            fnPtr: record_back as *mut c_void,
        }];
        // SAFETY: `env` is the live environment of this attached thread, `class` is the activity's
        // own class, and the function table entry is the JVM's own — the pointers are valid for the
        // whole call, and the entries the JVM copies stay valid while the library is loaded.
        let rc = unsafe {
            let raw = env.get_raw();
            let table = &*(*raw as *const jni::sys::JNINativeInterface_);
            match table.RegisterNatives {
                Some(register) => register(raw, class.as_raw(), methods.as_ptr(), methods.len() as i32),
                None => -1,
            }
        };
        Ok(rc == 0)
    });
    if !bound.unwrap_or(false) {
        log::warn!("android access: could not bind DeckCraftActivity.backInvoked; Android answers Back itself and the activity leaves");
    }
    bound.unwrap_or(false)
}

/// The app's package, as the manifest declares it (the settings intent's URI names it).
const PACKAGE: &str = "ai.storyteller.deckcraft";

/// The activity's JVM and activity reference, captured before the `AndroidApp` moves into
/// eframe. The activity owns both and lives as long as the app, so every later call is valid.
#[derive(Clone, Copy)]
pub struct AndroidAccess {
    vm: *mut jni::sys::JavaVM,
    activity: jni::sys::jobject,
}

// SAFETY: both pointers belong to the activity and are never freed while the app runs, which is
// the only time they are touched (on the single thread that runs the UI); jni's `from_raw`
// adopts them without owning them, so sharing them between the closures handed to the app is
// sound.
unsafe impl Send for AndroidAccess {}
unsafe impl Sync for AndroidAccess {}

/// Capture the handles from the activity. `None` (and a log) when no JVM is attached, in which
/// case the browser simply keeps its app-private root.
pub fn capture(app: &android_activity::AndroidApp) -> Option<AndroidAccess> {
    let vm = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    let activity = app.activity_as_ptr() as jni::sys::jobject;
    if vm.is_null() || activity.is_null() {
        log::warn!("android access: no JVM on the activity; \"All files access\" stays unavailable");
        return None;
    }
    Some(AndroidAccess { vm, activity })
}

impl AndroidAccess {
    /// Run one JNI session on this thread: attach to the activity's VM, hand `f` the environment,
    /// then make sure no Java exception is left pending (the JVM would otherwise rethrow it into
    /// unrelated Java code later — checking and clearing is allowed while an exception is
    /// pending). `None` when the VM could not be reached or `f` erred.
    fn with_env<T>(self, f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>) -> Option<T> {
        // SAFETY: `vm` was checked non-null in `capture` and belongs to the activity, which is
        // alive for the whole app; jni adopts it without taking ownership.
        let vm = unsafe { JavaVM::from_raw(self.vm).ok()? };
        // The UI thread is attached by the activity; attaching again is a no-op there, and this
        // attaches any other thread that asks. jni ties the environment to this borrow.
        let mut env = vm.attach_current_thread_permanently().ok()?;
        let out = f(&mut env).ok()?;
        if env.exception_check().unwrap_or(true) {
            log::warn!("android access: a JNI call threw; treated as failure");
            let _ = env.exception_clear();
            return None;
        }
        Some(out)
    }

    /// The activity object, for calls on it (permission checks, the settings intent).
    fn activity(self) -> JObject<'static> {
        // SAFETY: `activity` was checked non-null in `capture` and is owned by the activity,
        // alive for the whole app; jni adopts it without taking ownership.
        unsafe { JObject::from_raw(self.activity) }
    }

    /// The device's API level (`Build.VERSION.SDK_INT`), or `None` when the JVM can't answer.
    fn sdk_int(self) -> Option<i32> {
        self.with_env(|env| -> jni::errors::Result<i32> { env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i() })
    }

    /// Whether the call started cleanly: nothing threw on the spot (a pending exception means
    /// Android refused the intent or the permission call).
    fn started(env: &mut JNIEnv) -> jni::errors::Result<bool> {
        Ok(!env.exception_check().unwrap_or(true))
    }

    /// Whether shared storage may be read right now: "All files access" on Android 11+, the
    /// legacy runtime storage grant on Android 9. Android 10 answers never — scoped storage
    /// opened no shared tree, and this app targets Android 13+, so that one version keeps the
    /// browser in app storage.
    pub fn shared_storage_granted(self) -> bool {
        match self.sdk_int() {
            Some(28) => self
                .with_env(|env| -> jni::errors::Result<bool> {
                    // context.checkSelfPermission(READ_EXTERNAL_STORAGE): PERMISSION_GRANTED is 0.
                    let perm: JObject = env.new_string("android.permission.READ_EXTERNAL_STORAGE")?.into();
                    let act = self.activity();
                    let r = env.call_method(&act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?;
                    Ok(r.i()? == 0)
                })
                .unwrap_or(false),
            Some(29) => false,
            Some(level) if level >= 30 => self
                .with_env(|env| -> jni::errors::Result<bool> {
                    env.call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])?.z()
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Ask for shared-storage access: open *this app's* "All files access" settings page (Android
    /// 11+, where the user toggles the grant), or show the runtime permission dialog (Android 9).
    /// Returns whether Android started cleanly; a failure is only logged — the browser keeps its
    /// app-private root and can ask again on the next tap.
    pub fn ask_shared_storage_grant(self) -> bool {
        let asked = match self.sdk_int() {
            Some(level) if level >= 30 => self.with_env(|env| -> jni::errors::Result<bool> {
                // Uri.parse("package:ai.storyteller.deckcraft") + Intent(manage-page action, uri)
                // + activity.startActivity(intent): the settings screen that shows this app's
                // toggle. Android shows it; the app never grants itself.
                let pkg: JObject = env.new_string(format!("package:{PACKAGE}"))?.into();
                let uri =
                    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&pkg)])?.l()?;
                let action: JObject = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?.into();
                let intent = env.new_object(
                    "android/content/Intent",
                    "(Ljava/lang/String;Landroid/net/Uri;)V",
                    &[JValue::Object(&action), JValue::Object(&uri)],
                )?;
                let act = self.activity();
                env.call_method(&act, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
                Self::started(env)
            }),
            Some(28) => self.with_env(|env| -> jni::errors::Result<bool> {
                // The runtime permission dialog: requestPermissions(new String[]{read, write}, 42).
                let arr = env.new_object_array(2, "java/lang/String", JObject::null())?;
                for (i, perm) in ["android.permission.READ_EXTERNAL_STORAGE", "android.permission.WRITE_EXTERNAL_STORAGE"].into_iter().enumerate() {
                    let s = env.new_string(perm)?;
                    env.set_object_array_element(&arr, i as jni::sys::jsize, &s)?;
                }
                let act = self.activity();
                env.call_method(&act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(42)])?;
                Self::started(env)
            }),
            _ => None, // Android 10 (or no JVM): nothing to ask; the browser stays in app storage.
        };
        if asked != Some(true) {
            log::warn!("android access: could not ask for shared-storage access (the browser keeps its app-private root)");
        }
        asked == Some(true)
    }
}
