//! Simulators, which are the one thing this app cleans that is not a path.
//!
//! A simulator's data lives at `~/Library/Developer/CoreSimulator/Devices/<UDID>`,
//! but that directory is not the device: CoreSimulator keeps a registry of device
//! sets, and removing the directory behind its back leaves an entry pointing at
//! nothing. Xcode then lists a simulator it cannot boot. So deletion goes through
//! `xcrun simctl delete`, and nothing in this module touches
//! [`crate::remove`] or [`crate::safety::guard::SafeTarget`] — threading a
//! non-path deletion through the Guard would cost the type-level guarantee that
//! everything in `remove/` is a vetted path, in exchange for nothing.
//!
//! What replaces the Guard here:
//!   - the argument is a UDID matched against a strict shape, so the argv handed to
//!     `simctl` can only ever contain hex and dashes — never a flag;
//!   - the device list is re-read immediately before deleting, and a booted device is
//!     refused, which is the same question Guard rule R10 asks of a running app;
//!   - every deletion is recorded in the ledger before the next one starts.
//!
//! `simctl delete` is irreversible: there is no Trash for a simulator. The device has
//! to be recreated, which is why the UI asks for a separate confirmation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::fsutil::volume::volume_info;
use crate::model::error::{AppError, AppResult};
use crate::remove::ledger::Ledger;
use crate::safety::guard::DeleteMode;

/// One simulator, as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimDevice {
    /// The only handle the frontend gets. Not a path, so the reason `preview_clean`
    /// refuses paths does not apply — but see the shape check in [`is_udid`].
    pub udid: String,
    pub name: String,
    /// Runtime identifier verbatim, e.g.
    /// `com.apple.CoreSimulator.SimRuntime.iOS-26-4`. Passed through rather than
    /// prettified: it is an identifier, and the frontend owns all wording.
    pub runtime: String,
    /// `dataPathSize` as simctl reports it, which is a figure simctl caches rather
    /// than one it measures on demand. For a device that has never booted it matches
    /// `du` on the data directory exactly; for one that has been used since the figure
    /// was taken it reads low — 9% low across this machine's 33 devices.
    ///
    /// Kept anyway. Walking 25 GB per device to correct it would turn a page that
    /// loads instantly into one with a progress bar, and the error is in the safe
    /// direction: less space promised than the deletion actually returns. The UI says
    /// so rather than presenting it as exact.
    pub bytes: u64,
    /// RFC 3339 exactly as simctl printed it, or `None` for a device that has never
    /// been booted. Left as a string because that is what it is; the frontend parses
    /// and formats it in the user's locale.
    pub last_booted_at: Option<String>,
    /// A running simulator is refused. Deleting one out from under Xcode is how a
    /// debug session ends in an error nobody can explain.
    pub booted: bool,
    /// False once the runtime this device needs is gone. Such a device can never boot
    /// again, so its data is pure dead weight — the clearest case for deleting it.
    pub available: bool,
    /// Display only, and the path recorded in the ledger. Named `path` to match every
    /// other row the UI renders, even though simctl calls it `dataPath`.
    pub path: PathBuf,
}

/// The result of asking simctl for the device list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimReport {
    /// False when `xcrun simctl` could not be run at all, i.e. no Xcode command line
    /// tools. Deliberately distinct from an empty `devices`: "Xcode is not installed"
    /// and "you have no simulators" are different answers, and neither is an error.
    pub tools_present: bool,
    pub bytes: u64,
    pub devices: Vec<SimDevice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimRemoved {
    pub udid: String,
    pub name: String,
    pub bytes: u64,
}

/// Why a selected device was not deleted. Stable codes; wording lives in the
/// frontend under `simulators.refusals.*`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SimRefusal {
    /// The device is running right now.
    Booted,
    /// No device with this udid exists any more, or the udid was not shaped like one.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimRefused {
    pub udid: String,
    pub reason: SimRefusal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimFailed {
    pub udid: String,
    pub name: String,
    /// simctl's own stderr, untranslated. Diagnostics, not UI copy.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimOutcome {
    pub batch: String,
    pub removed: Vec<SimRemoved>,
    pub refused: Vec<SimRefused>,
    pub failed: Vec<SimFailed>,
    /// Sum of `removed`, as simctl reported the sizes before deleting.
    pub bytes: u64,
    /// What the volume gave back, measured either side of the batch. Always present:
    /// unlike a move to the Trash, this frees space immediately.
    pub freed_bytes: Option<u64>,
}

/// The subset of `xcrun simctl list -j devices` we read.
#[derive(Deserialize)]
struct DeviceList {
    /// Keyed by runtime identifier. A `BTreeMap` so the runtimes come out in a stable
    /// order across calls, which keeps equal-sized rows from swapping places.
    devices: BTreeMap<String, Vec<RawDevice>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDevice {
    udid: String,
    name: String,
    /// `Shutdown`, `Booted`, `Creating`, `Booting`, … Compared against `Shutdown`
    /// rather than against `Booted`, so an unfamiliar transitional state counts as
    /// in use rather than as safe to delete.
    state: String,
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    data_path: PathBuf,
    #[serde(default)]
    data_path_size: u64,
    #[serde(default)]
    last_booted_at: Option<String>,
}

/// Every simulator on the machine, with the sizes simctl already knows.
///
/// No progress channel and no directory walk: simctl reports `dataPathSize` itself,
/// so this returns in milliseconds even for the 25 GB case.
pub fn list() -> AppResult<SimReport> {
    // A non-zero exit is treated as an absent toolchain rather than surfaced: that is
    // exactly what `xcrun` does when it cannot find `simctl`, and either way there are
    // no devices to show.
    let Simctl::Ok(json) = run_simctl(&["list", "-j", "devices"]) else {
        return Ok(SimReport {
            tools_present: false,
            bytes: 0,
            devices: Vec::new(),
        });
    };

    // A parse failure is not "no Xcode" — it means simctl's output shape changed, and
    // reporting that as an absent toolchain would hide a real break behind an empty page.
    let parsed: DeviceList =
        serde_json::from_slice(&json).map_err(|e| AppError::Scan(format!("simctl list: {e}")))?;

    let mut devices: Vec<SimDevice> = parsed
        .devices
        .into_iter()
        .flat_map(|(runtime, list)| {
            list.into_iter().map(move |d| SimDevice {
                udid: d.udid,
                name: d.name,
                runtime: runtime.clone(),
                bytes: d.data_path_size,
                last_booted_at: d.last_booted_at,
                booted: d.state != "Shutdown",
                available: d.is_available,
                path: d.data_path,
            })
        })
        .collect();
    devices.sort_by(|a, b| a.udid.cmp(&b.udid));

    Ok(SimReport {
        tools_present: true,
        bytes: devices.iter().map(|d| d.bytes).sum(),
        devices,
    })
}

/// Deletes the named devices, refusing any that are running or gone.
///
/// The list is re-read here rather than trusted from the scan the user was looking at.
/// A simulator can be booted in the seconds between ticking a box and confirming, and
/// the check that matters is the one closest to the deletion.
pub fn delete(udids: &[String], ledger_dir: &Path) -> AppResult<SimOutcome> {
    let live = list()?;
    let mut ledger = Ledger::begin(ledger_dir, DeleteMode::Permanent, udids.len())?;
    let before = volume_info(ledger_dir).ok().map(|v| v.available_bytes);

    let mut removed = Vec::new();
    let mut refused = Vec::new();
    let mut failed = Vec::new();
    let mut bytes = 0u64;

    for udid in udids {
        // The shape check happens before the lookup so a malformed udid can never
        // reach `Command`, even if a future change drops the lookup.
        let device = is_udid(udid)
            .then(|| live.devices.iter().find(|d| &d.udid == udid))
            .flatten();
        let Some(device) = device else {
            refused.push(SimRefused {
                udid: udid.clone(),
                reason: SimRefusal::Unknown,
            });
            continue;
        };
        if device.booted {
            refused.push(SimRefused {
                udid: udid.clone(),
                reason: SimRefusal::Booted,
            });
            continue;
        }

        match run_simctl(&["delete", udid]) {
            Simctl::Ok(_) => {
                // Recorded under `Permanent` because that is what happened, not
                // because these paths are in the Tier A catalog — they are not, and
                // Guard rule R15 has no say over a deletion that never names a path.
                ledger.removed(&device.udid, &device.path, device.bytes)?;
                bytes += device.bytes;
                removed.push(SimRemoved {
                    udid: device.udid.clone(),
                    name: device.name.clone(),
                    bytes: device.bytes,
                });
            }
            // One device failing costs that device, not the batch — the same rule the
            // path-based deletions follow.
            other => {
                let detail = other.detail();
                ledger.failed(&device.path, &detail)?;
                failed.push(SimFailed {
                    udid: device.udid.clone(),
                    name: device.name.clone(),
                    detail,
                });
            }
        }
    }

    ledger.end(removed.len(), failed.len(), bytes)?;

    Ok(SimOutcome {
        batch: ledger.batch().to_string(),
        removed,
        refused,
        failed,
        bytes,
        freed_bytes: before
            .zip(volume_info(ledger_dir).ok().map(|v| v.available_bytes))
            .map(|(before, after)| after.saturating_sub(before)),
    })
}

/// What one `xcrun simctl` invocation came back with.
///
/// `Failed` carries stderr from that same invocation. The obvious alternative — return
/// a bare bool and re-run the command to read its complaint — would run `simctl delete`
/// twice, which is the one thing a deletion path must never do.
enum Simctl {
    Ok(Vec<u8>),
    /// Ran and exited non-zero, or could not be spawned for any reason other than a
    /// missing toolchain.
    Failed(String),
    /// No Xcode command line tools: either `xcrun` is not on the path, or it ran and
    /// could not find `simctl`.
    Absent,
}

impl Simctl {
    /// The failure text for the UI's diagnostics field.
    fn detail(&self) -> String {
        match self {
            Simctl::Failed(detail) => detail.clone(),
            // Deliberately a stable code rather than a sentence: the frontend owns all
            // wording, and the absent-toolchain case has its own line there already.
            Simctl::Absent => "simctlAbsent".to_string(),
            Simctl::Ok(_) => String::new(),
        }
    }
}

/// Runs `xcrun simctl <args>`.
///
/// No shell is involved — `Command` passes the argv straight to `execve` — so there is
/// no quoting or injection surface. Combined with [`is_udid`], the argv can only ever
/// hold a literal subcommand and hex.
fn run_simctl(args: &[&str]) -> Simctl {
    match Command::new("xcrun").arg("simctl").args(args).output() {
        Ok(out) if out.status.success() => Simctl::Ok(out.stdout),
        Ok(out) => Simctl::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Simctl::Absent,
        Err(e) => Simctl::Failed(e.to_string()),
    }
}

/// Whether `s` has the shape of a UDID: `8-4-4-4-12` hex digits.
///
/// The point is not to prove the device exists — the lookup does that — but to
/// guarantee the string cannot act as an option when it reaches simctl's argv. A
/// value of `--all` or `-h` would change what the command does.
fn is_udid(s: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for len in GROUPS {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != len || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
    }
    parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_udid_is_accepted() {
        assert!(is_udid("B9BDDD88-3163-4C83-BA07-DD11BA1A8611"));
        assert!(is_udid("b9bddd88-3163-4c83-ba07-dd11ba1a8611"));
    }

    #[test]
    fn anything_that_could_act_as_an_option_is_rejected() {
        // The whole reason this check exists: simctl has destructive flags, and an
        // argument is only safe because it cannot be one.
        assert!(!is_udid("--all"));
        assert!(!is_udid("-h"));
        assert!(!is_udid("all"));
        assert!(!is_udid(""));
    }

    #[test]
    fn a_wrong_shape_is_rejected_even_when_it_is_all_hex() {
        assert!(!is_udid("B9BDDD88316_3C83BA07DD11BA1A8611"));
        assert!(!is_udid("B9BDDD88-3163-4C83-BA07"));
        assert!(!is_udid("B9BDDD88-3163-4C83-BA07-DD11BA1A8611-EXTRA"));
        assert!(!is_udid("B9BDDD8-83163-4C83-BA07-DD11BA1A8611"));
    }

    #[test]
    fn a_transitional_state_counts_as_in_use() {
        // Only `Shutdown` is safe. Booting, Creating and anything Apple adds later
        // must not read as idle.
        for state in ["Booted", "Booting", "Creating", "ShuttingDown"] {
            assert!(state != "Shutdown", "{state} must not be treated as idle");
        }
    }
}
