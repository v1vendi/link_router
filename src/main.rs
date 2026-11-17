//! LinkRouter: a default-browser shim. macOS hands it each clicked link; it picks a
//! Chrome profile from ~/.config/linkrouter/rules (format: rules.example), opens the
//! link there, brings Chrome to the front and quits. Nothing stays resident.
//!
//! CLI check without opening anything:  LinkRouter route <url> [opener-bundle-id]
//! Debug log: create ~/Library/Logs/LinkRouter.log and every link is appended to it.

mod rules;

use std::path::PathBuf;
use std::process::Command;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationDelegate,
    NSApplicationLaunchIsDefaultLaunchKey, NSRunningApplication,
};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSArray, NSNotification, NSNumber, NSObject,
    NSObjectProtocol, NSString, NSURL,
};

const CHROME_ID: &str = "com.google.Chrome";
const KEY_SENDER_PID: u32 = u32::from_be_bytes(*b"spid");
/// Safety net: quit if launched for a link that never arrives.
const IDLE_QUIT_SECS: f64 = 30.0;

fn home(rel: &str) -> Option<PathBuf> {
    std::env::home_dir().map(|h| h.join(rel))
}

fn load_rules() -> Vec<rules::Rule> {
    home(".config/linkrouter/rules")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| rules::parse(&t))
        .unwrap_or_default()
}

fn log(msg: &str) {
    use std::io::Write;
    let file = home("Library/Logs/LinkRouter.log")
        .and_then(|p| std::fs::OpenOptions::new().append(true).open(p).ok());
    if let Some(mut f) = file {
        let t = std::time::UNIX_EPOCH
            .elapsed()
            .map_or(0.0, |d| d.as_secs_f64());
        let _ = writeln!(f, "{t:.3} {msg}");
    }
}

fn open(args: &[&str]) {
    if let Err(e) = Command::new("/usr/bin/open").args(args).status() {
        log(&format!("open failed: {e}"));
    }
}

fn open_link(url: &str, profile: Option<&str>) {
    let Some(p) = profile else {
        // Chrome's own handling: last active window, Chrome comes to the front.
        return open(&["-b", CHROME_ID, url]);
    };
    let flag = format!("--profile-directory={p}");
    let id = NSString::from_str(CHROME_ID);
    let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).firstObject();
    let Some(chrome) = running else {
        // Not running: this launch is the browser itself and LaunchServices focuses it.
        return open(&["-n", "-b", CHROME_ID, "--args", &flag, url]);
    };
    // Running: a second Chrome process forwards URL + profile to it and exits, but the
    // running Chrome doesn't activate itself. Activate it first so the profile window
    // comes forward when the tab opens; run the forwarder directly (not via LaunchServices,
    // which would activate it and hand focus back to the previous app when it exits) and
    // wait for it; then activate again in case the first request was refused.
    #[allow(deprecated)]
    let activate = || chrome.activateWithOptions(NSApplicationActivationOptions::empty());
    activate();
    let forwarded = chrome
        .executableURL()
        .and_then(|u| u.to_file_path())
        .map(|exe| Command::new(exe).args([&flag, url]).status());
    if !matches!(forwarded, Some(Ok(_))) {
        log(&format!(
            "direct forward failed ({forwarded:?}); using open"
        ));
        open(&["-g", "-W", "-n", "-b", CHROME_ID, "--args", &flag, url]);
    }
    activate();
}

/// Bundle id of the app that sent the current "open URL" Apple event (e.g. Slack).
fn opener_bundle_id() -> Option<String> {
    let event = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent()?;
    // msg_send! avoids pulling in objc2-core-services just for the AEKeyword type.
    let attr: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![&*event, attributeDescriptorForKeyword: KEY_SENDER_PID] };
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(attr?.int32Value())?;
    app.bundleIdentifier().map(|s| s.to_string())
}

/// True for a plain launch (e.g. double-click) as opposed to one for opening a link.
fn is_default_launch(n: &NSNotification) -> bool {
    let key = unsafe { NSApplicationLaunchIsDefaultLaunchKey };
    n.userInfo()
        .and_then(|info| info.objectForKey(key))
        .and_then(|f| f.downcast::<NSNumber>().ok())
        .is_some_and(|f| f.boolValue())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LinkRouterDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, n: &NSNotification) {
            if is_default_launch(n) {
                return self.terminate();
            }
            let app = NSApplication::sharedApplication(self.mtm());
            unsafe {
                let _: () = msg_send![&*app, performSelector: sel!(terminate:), withObject: Option::<&NSObject>::None, afterDelay: IDLE_QUIT_SECS];
            }
        }

        #[unsafe(method(application:openURLs:))]
        fn open_urls(&self, _app: &NSApplication, urls: &NSArray<NSURL>) {
            let opener = opener_bundle_id();
            let rules = load_rules();
            for url in urls.iter().filter_map(|u| u.absoluteString()) {
                let url = url.to_string();
                let profile = rules::route(&rules, &url, opener.as_deref());
                log(&format!("{url} opener={opener:?} profile={profile:?}"));
                open_link(&url, profile);
            }
            self.terminate();
        }
    }
);

impl Delegate {
    fn terminate(&self) {
        NSApplication::sharedApplication(self.mtm()).terminate(None);
    }

    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("route") {
        let Some(url) = args.get(2) else {
            eprintln!("usage: LinkRouter route <url> [opener-bundle-id]");
            std::process::exit(2);
        };
        let rules = load_rules();
        let profile = rules::route(&rules, url, args.get(3).map(String::as_str));
        println!("{}", profile.unwrap_or("(default)"));
        return;
    }

    let mtm = MainThreadMarker::new().expect("main thread");
    let delegate = Delegate::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
