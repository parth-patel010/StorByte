//! Light release hardening: strip easy strings tooling wins; raise bar for casual RE.
//! This is not DRM — a determined reverse engineer with a debugger can still analyze
//! a local binary. Keep crown-jewel logic on a server when absolute secrecy is required.

/// Best-effort anti-debug for release Windows builds. No-op in debug / non-Windows.
pub fn apply_release_hardening() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        anti_debug_windows();
    }
}

#[cfg(all(windows, not(debug_assertions)))]
fn anti_debug_windows() {
    use windows::Win32::System::Diagnostics::Debug::IsDebuggerPresent;

    // Bail early if a user-mode debugger is attached at startup.
    if unsafe { IsDebuggerPresent().as_bool() } {
        std::process::exit(1);
    }
}
