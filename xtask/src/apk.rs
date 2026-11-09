//! `cargo xtask apk`: DeckCraft's Android APK, including its Java activity glue.
//!
//! cargo-apk compiles Rust and packs the manifest, resources and `.so`; it compiles no Java.
//! Android's `android.app.NativeActivity` overrides no key method and no back method, so Back is
//! answered by the framework's own back dispatcher, which finishes the activity and leaves the
//! presentation in the background with a dialog still open. The APK therefore needs one activity
//! that answers Back as Escape (`apps/deckcraft-android/java/…/DeckCraftActivity.java`, the Android
//! glue exception named in AGENTS.md §3). This step runs cargo-apk, compiles that class with the
//! SDK's partner JDK's `javac`, turns it into `classes.dex` with `d8`, adds the dex to the APK
//! cargo-apk built, aligns it and re-signs it — cargo-apk's signature does not survive adding a
//! file.
//!
//! The keystore is resolved the way cargo-apk resolves it (`CARGO_APK_DEV_KEYSTORE`/`_PASSWORD` or
//! `CARGO_APK_RELEASE_KEYSTORE`/`_PASSWORD`, else the debug keystore for a debug build), so the
//! re-signed APK keeps the identity the build was signed with and one command covers both profiles.
//!
//! `cargo apk build` alone still works and produces an APK without the glue: Back would finish the
//! activity. Use this command for anything that goes to a device.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The Java glue's source tree, under the entry crate.
pub const JAVA_SRC: &str = "apps/deckcraft-android/java";
/// The activity the manifest names. A leading dot resolves it against the package, so the manifest
/// and the compiled class agree without repeating the package in both places.
pub const GLUE_ACTIVITY: &str = ".DeckCraftActivity";
/// Android's answer to Back, as the glue records it: DeckCraft's loop turns the record into Escape,
/// which dismisses a dialog, a menu or a cancellation on every host.
pub const BACK_RECORD: &str = "backInvoked";
/// The API 33 registration Android's back dispatcher needs: without it Android answers Back itself
/// and finishes the activity before the app ever sees the gesture.
pub const BACK_CALLBACK: &str = "registerOnBackInvokedCallback";
/// How the record crosses the Java/Rust boundary: JNI's name lookup cannot find this crate's symbol
/// from the app's class loader (the library is loaded by framework code), so the method must be
/// registered against the activity's own class — on a Tab S11 the lookup failure threw
/// `UnsatisfiedLinkError` inside the callback and Android killed the process mid-dialog.
pub const BACK_BINDING: &str = "RegisterNatives";
/// The entry crate's JNI module, which holds the binding and the record.
pub const ENTRY_JNI_SRC: &str = "apps/deckcraft-android/src/android_access.rs";
/// The entry crate's `android_main`, which must bind the record before the loop can receive it.
pub const ENTRY_MAIN: &str = "apps/deckcraft-android/src/lib.rs";
/// The entry crate's manifest, which carries the activity name and the min SDK the dex targets.
pub const ENTRY_MANIFEST: &str = "apps/deckcraft-android/Cargo.toml";

/// The APK cargo-apk writes, for a debug or release build.
pub fn apk_path(root: &Path, release: bool) -> PathBuf {
    root.join(if release { "target/release/apk/deckcraft.apk" } else { "target/debug/apk/deckcraft.apk" })
}

/// The `min_sdk_version` the entry crate asks for, so `d8`'s `--min-api` matches the manifest.
/// 28 is the floor when the metadata says nothing.
pub fn min_sdk(manifest: &str) -> u32 {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("min_sdk_version"))
        .find_map(|line| line.split('=').nth(1).and_then(|v| v.trim().parse::<u32>().ok()))
        .unwrap_or(28)
}

/// The SDK directory, as cargo-apk and the NDK resolve it.
fn sdk_dir() -> Result<PathBuf, String> {
    let home = std::env::var("ANDROID_HOME").or_else(|_| std::env::var("ANDROID_SDK_ROOT")).ok().filter(|s| !s.trim().is_empty());
    match home {
        Some(h) => {
            let p = PathBuf::from(h);
            if p.is_dir() { Ok(p) } else { Err(format!("ANDROID_HOME is not a directory: {}", p.display())) }
        }
        None => Err("ANDROID_HOME (or ANDROID_SDK_ROOT) is not set".to_string()),
    }
}

/// A directory entry whose name is `<major>.<minor>.<patch>` (a build-tools revision) or
/// `android-<api>` (a platform). Returns the leading numbers so the newest one can be picked.
fn version_of(name: &str) -> Option<Vec<u32>> {
    let head = name.strip_prefix("android-").unwrap_or(name);
    let parts: Vec<u32> = head.split('.').filter_map(|p| p.parse::<u32>().ok()).collect();
    if parts.is_empty() { None } else { Some(parts) }
}

/// The newest `build-tools/<revision>` directory: it carries `d8`, `aapt`, `zipalign` and
/// `apksigner`.
fn build_tools(sdk: &Path) -> Result<PathBuf, String> {
    let dir = sdk.join("build-tools");
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(v) = version_of(&entry.file_name().to_string_lossy()) else { continue };
        if best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, path));
        }
    }
    best.map(|(_, p)| p).ok_or_else(|| format!("{}/build-tools has no revision directory", sdk.display()))
}

/// The newest installed platform's `android.jar`: the classpath `javac` compiles the glue against
/// and the library `d8` links it against.
fn platform_jar(sdk: &Path) -> Result<PathBuf, String> {
    let dir = sdk.join("platforms");
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let Ok(entry) = entry else { continue };
        let jar = entry.path().join("android.jar");
        if !jar.is_file() {
            continue;
        }
        let Some(v) = version_of(&entry.file_name().to_string_lossy()) else { continue };
        if best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, jar));
        }
    }
    best.map(|(_, p)| p).ok_or_else(|| format!("{}/platforms has no android.jar (run sdkmanager)", sdk.display()))
}

/// Where `javac` is. A JDK beside the `java` the shell finds is the common case (Android Studio's
/// bundled JDK ships both), and the JRE-only OpenJDK packages ship neither, so the search goes
/// `JAVAC`, then `JAVA_HOME/bin/javac`, then PATH, then the directory PATH's `java` resolves to.
fn javac() -> Result<Command, String> {
    if let Some(v) = std::env::var("JAVAC").ok().filter(|s| !s.trim().is_empty()) {
        let p = PathBuf::from(v);
        if p.is_file() {
            return Ok(Command::new(p));
        }
        return Err(format!("JAVAC is not a file: {}", p.display()));
    }
    if let Some(home) = std::env::var("JAVA_HOME").ok().filter(|s| !s.trim().is_empty()) {
        let p = PathBuf::from(home).join("bin/javac");
        if p.is_file() {
            return Ok(Command::new(p));
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = path.split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect();
    for dir in &dirs {
        let p = dir.join("javac");
        if p.is_file() {
            return Ok(Command::new(p));
        }
    }
    for dir in &dirs {
        let java = dir.join("java");
        if !java.is_file() {
            continue;
        }
        // Follow the wrapper symlink to the real JVM directory and look for javac there.
        let Ok(real) = std::fs::canonicalize(&java) else { continue };
        if let Some(p) = real.parent().map(|d| d.join("javac"))
            && p.is_file()
        {
            return Ok(Command::new(p));
        }
    }
    Err("no javac found: install a JDK, or set JAVAC / JAVA_HOME. The activity glue must compile \
         into classes.dex, and a JRE-only OpenJDK package has no compiler."
        .to_string())
}

/// The keystore cargo-apk signed with, so re-signing keeps the identity of the profile asked for.
/// Debug falls back to Android's debug keystore, exactly as cargo-apk does.
fn keystore(release: bool) -> Result<(PathBuf, String), String> {
    let (ks_var, pw_var) = if release {
        ("CARGO_APK_RELEASE_KEYSTORE", "CARGO_APK_RELEASE_KEYSTORE_PASSWORD")
    } else {
        ("CARGO_APK_DEV_KEYSTORE", "CARGO_APK_DEV_KEYSTORE_PASSWORD")
    };
    let env_path = std::env::var(ks_var).ok().filter(|s| !s.trim().is_empty()).map(PathBuf::from);
    let env_pw = std::env::var(pw_var).ok().filter(|s| !s.trim().is_empty());
    match (env_path, env_pw) {
        (Some(p), Some(pw)) => {
            if !p.is_file() {
                return Err(format!("{ks_var} is not a file: {}", p.display()));
            }
            Ok((p, pw))
        }
        (Some(p), None) if !release => {
            // cargo-apk uses Android's default debug password; do the same rather than fail.
            Ok((p, "android".to_string()))
        }
        (Some(_), None) => Err(format!("{pw_var} is not set (a release build needs both)")),
        (None, _) if release => Err(format!("{ks_var} is not set: signing a release APK is the owner's decision (AGENTS.md §10)")),
        (None, _) => {
            let home =
                std::env::var("HOME").ok().filter(|s| !s.trim().is_empty()).ok_or("HOME is not set, so Android's debug keystore cannot be found")?;
            let p = PathBuf::from(home).join(".android/debug.keystore");
            if !p.is_file() {
                return Err(format!("no debug keystore at {}: run `cargo xtask apk` with {} set", p.display(), ks_var));
            }
            Ok((p, "android".to_string()))
        }
    }
}

fn tool(tools: &Path, name: &str) -> Result<Command, String> {
    let bin = tools.join(name);
    if !bin.is_file() {
        return Err(format!("{name} not found in {} (install it with sdkmanager)", tools.display()));
    }
    Ok(Command::new(bin))
}

fn exec(mut cmd: Command, what: &str) -> Result<(), String> {
    // Never echo a command that carries a keystore password: a password is a secret (AGENTS.md §10).
    if what.contains("keystore") {
        eprintln!("$ {what}");
    } else {
        eprintln!("$ {cmd:?}");
    }
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

/// cargo-apk's own build, so the Rust library, manifest, resources and signing profile are the ones
/// the entry crate's metadata asks for.
fn build_apk(release: bool) -> Result<(), String> {
    let mut cmd = crate::cargo();
    cmd.args(["apk", "build", "-p", "deckcraft-android"]);
    if release {
        cmd.arg("--release");
    }
    exec(cmd, "cargo apk build")
}

/// Compile the glue against `android.jar`, returning the `.class` files `d8` consumes. `d8` takes
/// class files, not a directory, so every one is listed.
fn compile_java(root: &Path, jar: &Path) -> Result<Vec<PathBuf>, String> {
    let src = root.join(JAVA_SRC);
    if !src.is_dir() {
        return Err(format!("no Java glue at {}", src.display()));
    }
    let mut sources = Vec::new();
    walk(&src, "java", &mut sources);
    if sources.is_empty() {
        return Err(format!("{} holds no .java files", src.display()));
    }
    let classes = crate::root().join("target/android-java");
    let _ = std::fs::remove_dir_all(&classes);
    std::fs::create_dir_all(&classes).map_err(|e| format!("create {}: {e}", classes.display()))?;
    let mut cmd = javac()?;
    cmd.args(["--release", "8", "-classpath", &jar.to_string_lossy(), "-d", &classes.to_string_lossy()]);
    cmd.args(&sources);
    exec(cmd, "javac (Android activity glue)")?;
    let mut compiled = Vec::new();
    walk(&classes, "class", &mut compiled);
    if compiled.is_empty() {
        return Err(format!("javac wrote no .class files under {}", classes.display()));
    }
    Ok(compiled)
}

/// Collect files under `dir` with the given extension, in a stable order.
fn walk(dir: &Path, ext: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, ext, files);
        } else if path.extension().is_some_and(|e| e == ext) {
            files.push(path);
        }
    }
}

/// `d8` turns the compiled classes into one `classes.dex` at the manifest's min SDK.
fn compile_dex(classes: &[PathBuf], jar: &Path, tools: &Path, min_api: u32) -> Result<PathBuf, String> {
    let out = crate::root().join("target/android-dex");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let mut cmd = tool(tools, "d8")?;
    cmd.args(["--min-api", &min_api.to_string(), "--lib", &jar.to_string_lossy(), "--output", &out.to_string_lossy()]);
    cmd.args(classes);
    exec(cmd, "d8 (Android dex)")?;
    let dex = out.join("classes.dex");
    if !dex.is_file() {
        return Err(format!("d8 produced no {}", dex.display()));
    }
    Ok(dex)
}

/// Add the dex to the APK cargo-apk built. `aapt add` keeps the entries already in the ZIP and
/// stores the file's path relative to its working directory, so it runs beside the dex and is given
/// only the name — `classes.dex` at the archive's root is what Android installs.
///
/// `-0 ""` stores it uncompressed: Android's package installer only treats an APK as carrying code
/// when `classes.dex` is stored, not deflated — a deflated dex leaves the class loader's path list
/// empty and the activity fails to instantiate (`ClassNotFoundException`). ndk-build passes the same
/// option when compression is disabled.
fn add_dex(tools: &Path, apk: &Path, dex: &Path) -> Result<(), String> {
    if !apk.is_file() {
        return Err(format!("no APK at {}", apk.display()));
    }
    let mut cmd = tool(tools, "aapt")?;
    cmd.current_dir(dex.parent().unwrap_or(Path::new(".")));
    cmd.args(["add", "-0", ""]).arg(apk).arg(dex.file_name().unwrap_or(std::ffi::OsStr::new("classes.dex")));
    exec(cmd, "aapt add classes.dex")
}

/// Whether `classes.dex` is stored uncompressed in the APK, which is what makes Android load it.
/// Reads the ZIP's local file headers, so no SDK tool is needed. A local header is 30 bytes
/// (`PK\0x03\0x04`, version, flags, method, time, date, crc, sizes, name and extra lengths) and its
/// filename follows it, so a name found in the archive is 30 bytes past the header's signature and
/// the compression method is the 16-bit field 8 bytes into that header. The central directory's
/// entry is 46 bytes long and starts `PK\0x01\0x02`, so its copy of the name never matches here.
fn dex_is_stored(apk: &Path) -> Result<bool, String> {
    let name = b"classes.dex";
    let bytes = std::fs::read(apk).map_err(|e| format!("read {}: {e}", apk.display()))?;
    let mut found = false;
    let mut at = 0;
    while let Some(i) = bytes[at..].windows(name.len()).position(|w| w == name) {
        let start = at + i;
        if start >= 30 && bytes[start - 30..start - 26] == [0x50, 0x4b, 0x03, 0x04] {
            let Some(method) = bytes.get(start - 22..start - 20) else {
                return Err(format!("{}: truncated ZIP local header", apk.display()));
            };
            found = true;
            if u16::from_le_bytes([method[0], method[1]]) != 0 {
                return Ok(false);
            }
        }
        at = start + name.len();
    }
    Ok(found)
}

/// Align the APK in place. `zipalign` takes an input and a separate output, so the APK is copied,
/// the copy is aligned into the APK's own path and the copy is removed: if aligning fails, the APK
/// on disk is still the one cargo-apk signed.
fn align(tools: &Path, apk: &Path) -> Result<(), String> {
    let unaligned = apk.with_extension("apk-unaligned");
    std::fs::copy(apk, &unaligned).map_err(|e| format!("copy {} for aligning: {e}", apk.display()))?;
    let mut cmd = tool(tools, "zipalign")?;
    cmd.args(["-f", "-v", "4"]).arg(&unaligned).arg(apk);
    let result = exec(cmd, "zipalign");
    let _ = std::fs::remove_file(&unaligned);
    result
}

fn sign(tools: &Path, apk: &Path, ks: &(PathBuf, String)) -> Result<(), String> {
    let mut cmd = tool(tools, "apksigner")?;
    cmd.args(["sign", "--ks", &ks.0.to_string_lossy(), &format!("--ks-pass=pass:{}", ks.1)]);
    cmd.arg(apk);
    exec(cmd, "apksigner sign (keystore password not echoed)")
}

fn verify(tools: &Path, apk: &Path) -> Result<(), String> {
    let mut cmd = tool(tools, "apksigner")?;
    cmd.args(["verify", "--print-certs"]).arg(apk);
    exec(cmd, "apksigner verify")
}

/// Whether the APK carries `classes.dex`, read from the ZIP's stored entry names — those are not
/// compressed, so no SDK tool is needed.
pub fn apk_has_dex(apk: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(apk).map_err(|e| format!("read {}: {e}", apk.display()))?;
    Ok(bytes.windows(b"classes.dex".len()).any(|w| w == b"classes.dex"))
}

/// What the built manifest must say: it names the activity, and it does not claim the APK carries
/// no code. `android:hasCode="false"` makes Android hand the activity an empty dex path list, so the
/// `classes.dex` holding the Back answer is never loaded (`ClassNotFoundException`, `DexPathList[[]]`).
fn check_manifest(xml: &str) -> Result<(), String> {
    if !xml.contains(GLUE_ACTIVITY) {
        return Err(format!("the built manifest does not name {GLUE_ACTIVITY}"));
    }
    if xml.contains("hasCode=\"false\"") {
        return Err("the manifest says android:hasCode=\"false\", so Android loads no code from the APK and \
             the activity in classes.dex cannot be instantiated: set has_code = true under \
             [package.metadata.android.application] in {ENTRY_MANIFEST}"
            .to_string());
    }
    Ok(())
}
/// What must be true for Back to answer Escape: the manifest names the activity and says the APK
/// carries code, the glue records Back, catches the call, binds it over JNI before the loop runs and
/// never finishes the activity, and the built APK carries the dex uncompressed.
fn check_glue(root: &Path, release: bool) -> Result<(), String> {
    let toml = std::fs::read_to_string(root.join(ENTRY_MANIFEST)).map_err(|e| format!("read {}: {e}", root.join(ENTRY_MANIFEST).display()))?;
    if !toml.contains(&format!("name = \"{GLUE_ACTIVITY}\"")) {
        return Err(format!(
            "{ENTRY_MANIFEST} must name the activity {GLUE_ACTIVITY}, or Android loads plain NativeActivity and Back finishes the presentation"
        ));
    }
    if !toml.contains("has_code = true") {
        return Err(format!(
            "{ENTRY_MANIFEST} must set has_code = true: cargo-apk writes android:hasCode=\"false\", \
             which tells Android the APK has no Java code, so the dex holding Back's answer is never loaded"
        ));
    }
    let java = std::fs::read_to_string(root.join(JAVA_SRC).join("ai/storyteller/deckcraft/DeckCraftActivity.java"))
        .map_err(|e| format!("read the activity glue: {e}"))?;
    if !java.contains("KEYCODE_BACK") || !java.contains(BACK_RECORD) {
        return Err(format!("the activity glue must answer KEYCODE_BACK by recording it as {BACK_RECORD} for DeckCraft's loop"));
    }
    if !java.contains(BACK_CALLBACK) {
        return Err(format!(
            "the activity glue must call {BACK_CALLBACK}: Android 13 and newer decide Back themselves \
             and finish the activity before the app sees it"
        ));
    }
    if java.contains("finish(") {
        return Err("the activity glue must never finish the activity: Back is DeckCraft's Escape".to_string());
    }
    // An exception thrown by the callback reaches Android's message queue, and Android kills the
    // process: the presentation would be lost, so the glue catches it and Back does nothing instead.
    if !java.contains("catch (") {
        return Err("the activity glue must catch the call to backInvoked: an exception from the back callback \
             reaches Android's message queue and Android kills the process, losing the presentation"
            .to_string());
    }
    let jni = std::fs::read_to_string(root.join(ENTRY_JNI_SRC)).map_err(|e| format!("read the entry crate's JNI module: {e}"))?;
    if !jni.contains(BACK_BINDING) || !jni.contains(BACK_RECORD) {
        return Err(format!(
            "{ENTRY_JNI_SRC} must bind the activity's {BACK_RECORD} with {BACK_BINDING}: JNI's name \
             lookup searches only the class loader that defined the class, and NativeActivity loads \
             this library from framework code, so an unbound call throws UnsatisfiedLinkError"
        ));
    }
    let main = std::fs::read_to_string(root.join(ENTRY_MAIN)).map_err(|e| format!("read the entry crate's main: {e}"))?;
    if !main.contains("register_back") {
        return Err(format!("{ENTRY_MAIN} must bind Back's record (register_back) before the loop runs, or the first gesture throws"));
    }
    let apk = apk_path(root, release);
    let manifest = apk.parent().map(|p| p.join("AndroidManifest.xml")).unwrap_or_else(|| apk.clone());
    let xml = std::fs::read_to_string(&manifest).map_err(|e| format!("read {}: {e}", manifest.display()))?;
    check_manifest(&xml)?;
    if !apk_has_dex(&apk)? {
        return Err("the APK has no classes.dex: the Back answer would not be installed".to_string());
    }
    if !dex_is_stored(&apk)? {
        return Err("classes.dex is compressed in the APK: Android only loads a stored dex, so the activity would not instantiate".to_string());
    }
    Ok(())
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let release = args.contains(&"--release");
    let sdk = sdk_dir()?;
    let tools = build_tools(&sdk)?;
    let jar = platform_jar(&sdk)?;
    let toml = std::fs::read_to_string(root.join(ENTRY_MANIFEST)).map_err(|e| format!("read {}: {e}", root.join(ENTRY_MANIFEST).display()))?;
    build_apk(release)?;
    let apk = apk_path(root, release);
    let classes = compile_java(root, &jar)?;
    let dex = compile_dex(&classes, &jar, &tools, min_sdk(&toml))?;
    add_dex(&tools, &apk, &dex)?;
    align(&tools, &apk)?;
    let ks = keystore(release)?;
    sign(&tools, &apk, &ks)?;
    verify(&tools, &apk)?;
    check_glue(root, release)?;
    println!("{}: APK with the Back answer built, dex'd, aligned, signed and verified", apk.display());
    if args.contains(&"--install") {
        let adb = std::env::var("ADB")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                let p = sdk.join("platform-tools/adb");
                p.is_file().then_some(p)
            })
            .ok_or("no adb: set ADB or install platform-tools")?;
        let serial = args.iter().position(|a| *a == "--install").and_then(|i| args.get(i + 1)).filter(|a| !a.starts_with("--"));
        let mut cmd = Command::new(adb);
        if let Some(serial) = serial {
            cmd.args(["-s", serial]);
        }
        cmd.args(["install", "-r"]).arg(&apk);
        exec(cmd, "adb install")?;
        println!("installed on the device");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entry crate must name DeckCraft's activity, not plain NativeActivity: that is what makes
    /// Back answer Escape. Fails before the manifest change lands.
    #[test]
    fn the_entry_crate_names_the_back_answer_activity() {
        let toml = std::fs::read_to_string(crate::root().join(ENTRY_MANIFEST)).unwrap();
        assert!(toml.contains(&format!("name = \"{GLUE_ACTIVITY}\"")), "the activity must be {GLUE_ACTIVITY}, got:\n{toml}");
    }

    /// The glue answers Back by recording it and never finishes the activity, and it registers the
    /// callback Android 13+ needs. Fails while no Java exists.
    #[test]
    fn the_glue_records_back_and_never_finishes() {
        let java = std::fs::read_to_string(crate::root().join(JAVA_SRC).join("ai/storyteller/deckcraft/DeckCraftActivity.java")).unwrap();
        assert!(java.contains("KEYCODE_BACK"), "the glue must recognise KEYCODE_BACK");
        assert!(java.contains(BACK_RECORD), "the glue must record Back as {BACK_RECORD} for DeckCraft's loop");
        assert!(java.contains(BACK_CALLBACK), "Android 13+ decides Back itself, so a callback must be registered");
        assert!(java.contains("onBackPressed"), "the legacy back request must be answered too");
        assert!(!java.contains("finish("), "the glue must never finish the activity");
    }

    /// The glue must catch the record call: an exception thrown inside Android's back callback
    /// reaches the activity's message queue and Android kills the process, which loses the
    /// presentation — the crash this arc is meant to remove. Fails while the call goes straight
    /// through.
    #[test]
    fn the_glue_catches_an_unbound_back_record() {
        let java = std::fs::read_to_string(crate::root().join(JAVA_SRC).join("ai/storyteller/deckcraft/DeckCraftActivity.java")).unwrap();
        assert!(java.contains("catch ("), "an unbound backInvoked must be caught, not left to crash the activity");
    }

    /// The record must be bound to the activity's class over JNI, and bound before the loop starts.
    /// JNI's name lookup searches only the class loader that defined the class, and `NativeActivity`
    /// loads this crate's library from framework code, so the exported symbol is never found: on the
    /// tablet the callback threw `UnsatisfiedLinkError` and the process died. Fails while
    /// `android_access` relies on the name lookup alone and `android_main` never registers it.
    #[test]
    fn the_back_record_is_bound_over_jni_before_the_loop_can_receive_it() {
        let jni = std::fs::read_to_string(crate::root().join(ENTRY_JNI_SRC)).unwrap();
        assert!(jni.contains(BACK_BINDING), "the record must be bound with {BACK_BINDING}, got:\n{jni}");
        assert!(jni.contains(BACK_RECORD), "the binding must name {BACK_RECORD}");
        let main = std::fs::read_to_string(crate::root().join(ENTRY_MAIN)).unwrap();
        assert!(main.contains("register_back"), "android_main must bind Back before run_android, got:\n{main}");
    }

    /// The entry crate must say the APK carries code. cargo-apk's default `android:hasCode="false"`
    /// makes Android give the activity an empty dex path list, so `classes.dex` is never loaded — the
    /// tablet crash this arc was chasing. Fails before `has_code = true` lands.
    #[test]
    fn the_entry_crate_declares_that_the_apk_carries_code() {
        let toml = std::fs::read_to_string(crate::root().join(ENTRY_MANIFEST)).unwrap();
        assert!(toml.contains("has_code = true"), "the application must set has_code = true, got:\\n{toml}");
    }

    /// A manifest that names the activity but claims no code is rejected; one that says it carries
    /// code is accepted. Fails while `check_manifest` only checks the activity name.
    #[test]
    fn a_manifest_claiming_no_code_is_rejected() {
        let no_code = r#"<application android:hasCode="false" android:name=".DeckCraftActivity">"#;
        let with_code = r#"<application android:hasCode="true" android:name=".DeckCraftActivity">"#;
        assert!(check_manifest(no_code).is_err(), "hasCode=\"false\" means Android loads no dex");
        assert!(check_manifest(with_code).is_ok());
        assert!(check_manifest(r#"<application android:hasCode="true">"#).is_err(), "the activity must be named");
    }

    /// The dex's min SDK comes from the manifest, so `d8` cannot emit code the manifest forbids.
    #[test]
    fn the_dex_targets_the_manifests_min_sdk() {
        assert_eq!(min_sdk("min_sdk_version = 28"), 28);
        assert_eq!(min_sdk("min_sdk_version = 33"), 33);
        assert_eq!(min_sdk("[package.metadata.android.sdk]\n"), 28, "no min SDK means the floor");
    }

    /// A ZIP archive holding `name` twice — once in its local file header and once in the central
    /// directory entry — with no real data: enough for `dex_is_stored` to read the header it needs
    /// and to ignore the central directory's copy.
    fn zip_with(name: &str, method: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // local header signature
        b.extend_from_slice(&[0x20, 0x00]); // version needed
        b.extend_from_slice(&[0x00, 0x00]); // flags
        b.extend_from_slice(&method.to_le_bytes()); // compression method
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // modification time and date
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // crc
        b.extend_from_slice(&[4, 0, 0, 0]); // compressed size
        b.extend_from_slice(&[4, 0, 0, 0]); // uncompressed size
        b.extend_from_slice(&[name.len() as u8, 0x00]); // filename length
        b.extend_from_slice(&[0x00, 0x00]); // extra length
        b.extend_from_slice(name.as_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // central directory signature
        b.extend_from_slice(&[0x20, 0x00, 0x20, 0x00]); // version made by, version needed
        b.extend_from_slice(&[0x00, 0x00]); // flags
        b.extend_from_slice(&method.to_le_bytes()); // compression method
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // modification time and date
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // crc
        b.extend_from_slice(&[4, 0, 0, 0]); // compressed size
        b.extend_from_slice(&[4, 0, 0, 0]); // uncompressed size
        b.extend_from_slice(&[name.len() as u8, 0x00]); // filename length
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // extra, comment, disk
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // internal and external attributes
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // attributes
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // local header offset
        b.extend_from_slice(name.as_bytes());
        b
    }

    /// Android's installer only treats an APK as carrying code when `classes.dex` is stored
    /// uncompressed: a deflated dex leaves the class loader's path list empty and Back's activity
    /// fails to instantiate. Fails while `dex_is_stored` reads the wrong header field.
    #[test]
    fn android_loads_only_a_stored_classes_dex() {
        let dir = crate::root().join("target/xtask-dex-check");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stored = dir.join("stored.apk");
        std::fs::write(&stored, zip_with("classes.dex", 0)).unwrap();
        let deflated = dir.join("deflated.apk");
        std::fs::write(&deflated, zip_with("classes.dex", 8)).unwrap();
        assert!(dex_is_stored(&stored).unwrap(), "a stored classes.dex is code Android loads");
        assert!(!dex_is_stored(&deflated).unwrap(), "a deflated classes.dex is not");
        let no_dex = dir.join("no-dex.apk");
        std::fs::write(&no_dex, zip_with("readme.txt", 0)).unwrap();
        assert!(!dex_is_stored(&no_dex).unwrap(), "an APK with no dex has nothing to store");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `cargo apk build` alone has no dex, so the README must point builds at this command.
    #[test]
    fn the_readme_documents_the_apk_step() {
        let readme = std::fs::read_to_string(crate::root().join("README.md")).unwrap();
        assert!(readme.contains("cargo xtask apk"), "README must tell Android builds to use cargo xtask apk");
    }
}
