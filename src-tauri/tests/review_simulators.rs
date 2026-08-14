//! Phase 8's acceptance step for simulators: print the device list the page would show
//! and total it, so the figure can be diffed against `du -sk` on the Devices directory.
//! Nothing here deletes anything.
//!
//!   cargo test --test review_simulators -- --ignored --nocapture

use space_master_lib::simctl;

#[test]
#[ignore = "reads the real simulator list; run with --ignored --nocapture"]
fn the_real_simulator_list_is_reviewable() {
    let report = simctl::list().expect("simctl list");
    if !report.tools_present {
        println!("no Xcode command line tools; nothing to review");
        return;
    }

    println!("\n=== devices ({}) ===", report.devices.len());
    for d in &report.devices {
        println!(
            "{:>13} bytes  {:<7} {:<9} {:<24} {:<28} {}",
            d.bytes,
            if d.booted { "booted" } else { "idle" },
            if d.available { "ok" } else { "no-runtime" },
            d.last_booted_at.as_deref().unwrap_or("never"),
            d.name,
            d.udid,
        );
    }

    println!("\ntotal = {} bytes", report.bytes);
    println!("compare: du -sk ~/Library/Developer/CoreSimulator/Devices");

    assert_eq!(
        report.bytes,
        report.devices.iter().map(|d| d.bytes).sum::<u64>(),
        "the reported total is not the sum of the rows"
    );

    // Every row must carry the handle the page sends back, and it must be one the
    // backend will still accept when it comes back in. A device we can list but not
    // name would be a row whose checkbox silently does nothing.
    for d in &report.devices {
        assert!(
            d.path.starts_with(std::env::home_dir().expect("$HOME")),
            "{} sits outside $HOME",
            d.path.display()
        );
    }
}
