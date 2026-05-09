/// Post-setup discovery hint for the dashboard Explain tab.
pub(crate) fn print_explain_discovery_hint() {
    println!();
    println!("Explain configured. Open the dashboard and use the Explain tab:");
    println!("  synrepo dashboard");
    println!(
        "Then press a for the whole repo, c for recent changes, f for folders, or g for a target."
    );
}
