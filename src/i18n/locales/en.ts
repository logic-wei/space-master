/**
 * Base locale. Every other locale is type-checked against this shape, so a
 * missing or misspelled key is a compile error rather than a runtime fallback.
 *
 * The keys under `catalog` mirror the Rust catalog tables. Adding a row there
 * without adding wording here shows the raw id in the UI — which is deliberate for
 * the entries discovered at scan time, and a mistake for the ones we wrote down.
 */
export const en = {
  app: {
    name: 'SpaceMaster',
    tagline: 'Reclaim disk space on macOS',
  },
  language: {
    label: 'Language',
    'en': 'English',
    'zh-CN': '简体中文',
  },
  volume: {
    title: 'Data volume',
    loading: 'Reading disk information…',
    available: 'Available',
    used: 'Used',
    total: 'Total',
    usedPercent: '{{percent}}% used',
  },
  quick: {
    title: 'One-click clean',
    intro:
      'Caches the owning tool rebuilds by itself. Losing one costs a download, never your work.',
    empty: 'Nothing found to clean.',
  },
  dev: {
    title: 'Developer caches',
    intro:
      'Package registries, build caches and downloaded models. Nothing here is your work, but rebuilding one costs time — so these move to the Trash rather than being deleted outright.',
    empty: 'No developer caches found.',
    unknownCache:
      'Something under ~/.cache. We do not know which tool owns it, so check when it was last used before deciding.',
  },
  xcode: {
    title: 'Xcode',
    intro:
      'Device symbols, build output and archives. Rows are named by Xcode, not by us — read the description to see what a row is before ticking it. Everything here moves to the Trash.',
    empty: 'No Xcode caches found.',
  },
  simulators: {
    title: 'Simulators',
    intro:
      'Each simulator keeps its own copy of everything ever installed on it. Deleting one removes the device itself — Xcode will no longer list it, and you recreate it from Window ▸ Devices and Simulators.',
    empty: 'No simulators found.',
    noTools:
      'The Xcode command line tools were not found, so simulators cannot be listed. Install Xcode, or run xcode-select --install.',
    neverBooted: 'never started',
    running: 'Running — quit the Simulator app to delete it',
    unavailable: 'Its runtime is no longer installed, so this device cannot start.',
    sizeNote:
      'Sizes come from simctl, which reports a figure it recorded earlier rather than measuring now. A device you have used since then is larger than it says, so deleting one usually frees a little more than the number above.',
    delete: 'Delete simulators',
    deleteHint: 'Select at least one simulator to continue.',
    /** The separate confirmation this page needs and the catalog pages do not. */
    confirmTitle: 'Delete {{count}} simulators?',
    confirmBody:
      'This runs xcrun simctl delete. There is no Trash for a simulator: the device and everything installed on it are gone, and recreating it starts from empty.',
    confirm: 'Delete permanently',
    cancel: 'Keep them',
    working: 'Deleting…',
    removed: 'Deleted',
    freed: 'Space freed',
    freedNote:
      'Space freed is measured on the volume, so it is the figure `df` agrees with. It usually comes out above the reported size, because simctl reports a size it recorded before the device was last used.',
    nothingRemoved: 'No simulators were deleted.',
    refusedTitle: 'Not deleted',
    failedTitle: 'simctl reported an error',
    refusals: {
      booted: 'It was running by the time we got to it.',
      unknown: 'It no longer exists.',
    },
    done: 'Done',
  },
  orphans: {
    title: 'Leftovers',
    intro:
      'Caches, settings and containers named after software that is no longer installed. Every row is a judgement, so the evidence is shown rather than a verdict — and everything here moves to the Trash.',
    empty: 'No data was found that could belong to uninstalled software.',
    unreliable:
      'Leftover detection is switched off because the list of installed apps came out incomplete: {{named}} were identified and {{unnamed}} could not be. One missing app would make all of its data look abandoned, so no answer is better than a wrong one.',
    /** Rows that can be ticked, i.e. everything outside the protected bucket. */
    offered: 'On offer',
    rowCount: '{{count}} rows',
    placeCount: '{{count}} locations',
    notMeasured: 'not measured',
    showPlaces: 'Where it is',
    hidePlaces: 'Hide locations',
    reveal: 'Show in Finder',
    trashNote:
      'These move to the Trash, never deleted outright — a wrong guess here costs you a trip to the Trash, not your settings.',
    needsAccess:
      'Some of these are app containers, which macOS protects. They can be measured without Full Disk Access but not moved to the Trash — attempting it fails with a permission error partway through.',
    buckets: {
      likely: 'Almost certainly leftovers',
      possible: 'Probably leftovers',
      unclear: 'Needs your judgement',
      keep: 'Protected',
    },
    bucketNotes: {
      likely: 'Nothing argues for keeping these, so they start ticked.',
      possible:
        'The evidence points one way but not conclusively. Open a row to see where the data is before ticking it.',
      unclear:
        'Something here argues both ways. These cannot be ticked from this page — look at them in Finder and decide for yourself.',
      keep: 'Not on offer. Shown anyway, so you can see what was checked and why it was ruled out.',
    },
    /** Why a row is not on offer. */
    protected: {
      system: 'It belongs to macOS or to software Apple ships.',
      ownData: 'It is SpaceMaster’s own data.',
      installed: 'The app that owns it is installed.',
      installedFamily:
        'An installed app is part of this family, and shared containers are named after the family rather than the app.',
      launchdJob: 'A background job is registered under this name.',
      running: 'A process from this bundle is running right now.',
    },
    /** Facts about a row, shown as chips instead of a score. */
    evidence: {
      unusedOver180d: 'untouched for 6 months',
      unusedOver1y: 'untouched for a year',
      unusedOver2y: 'untouched for 2 years',
      recentActivity: 'written to this month',
      ageUnknown: 'age unknown',
      manyLocations: 'in several locations',
      onlyPreferences: 'only a settings file',
      standardId: 'standard bundle id',
      shortId: 'unusually short id',
      sameVendor: 'same vendor as an installed app',
      holdsDatabase: 'holds a database',
      large: 'large',
      tiny: 'a few kilobytes',
    },
    /** The directory under ~/Library a row was found in. Apple's names, not ours. */
    where: {
      caches: 'Caches',
      preferences: 'Preferences',
      applicationSupport: 'Application Support',
      containers: 'Container',
      groupContainers: 'Group Container',
      savedState: 'Saved window state',
      logs: 'Logs',
      webKit: 'WebKit data',
      httpStorages: 'HTTP storage',
      applicationScripts: 'Application Scripts',
    },
  },
  /**
   * Wording for a class of rows discovered by listing a directory. Their ids are
   * build numbers and project hashes, so this is the only description they get.
   */
  notes: {
    deviceSupport:
      'Symbols for one iOS version, copied from a device Xcode has debugged. Xcode extracts them again the next time that device is connected, which takes several minutes.',
    derivedData:
      'Build output, indexes and module caches for one project. The next build is a clean build; no source is involved.',
    archives:
      'A build you archived, including its dSYMs. Delete it and crash reports from that build can no longer be symbolized — keep the ones you shipped.',
  },
  /** Strings shared by every scan screen. */
  scan: {
    scan: 'Scan',
    rescan: 'Scan again',
    scanning: 'Scanning…',
    scanningFound: 'Scanning… {{size}} so far',
    cancel: 'Cancel',
    cancelled: 'Scan cancelled. The numbers below cover only what finished.',
    total: 'Reclaimable',
    selected: 'Selected',
    selectAll: 'Select all',
    selectNone: 'Clear selection',
    review: 'Review clean plan',
    reviewHint: 'Select at least one item to continue.',
    alreadyEmpty: 'Already empty',
    sizeUnknown: 'Size unknown',
    needsPermission:
      'This folder cannot be read without Full Disk Access, so its size is unknown.',
    needsAccess:
      'A folder here cannot be read without Full Disk Access. macOS allows moving things into the Trash without it but not listing what is already there, so the size shows as unknown.',
    fileCount: '{{count}} files',
    lastUsed: 'used {{when}}',
    scopeChildren: 'Contents are removed; the folder itself stays.',
    issues: '{{count}} entries were skipped while measuring',
    copyCommand: 'Copy command',
    copied: 'Copied',
  },
  plan: {
    title: 'Clean plan',
    confirmTrash:
      'These items move to the Trash. You can put them back from Finder, and no space is freed until you empty it.',
    confirmPermanent: 'These items are deleted permanently. This cannot be undone.',
    mode: 'Method',
    modeTrash: 'Move to Trash',
    modePermanent: 'Delete permanently',
    accepted: 'Would be deleted',
    itemCount: '{{count}} items',
    nothingAccepted: 'Nothing in this selection can be deleted.',
    rejected: 'Refused by a safety rule',
    noneRejected: 'Nothing was refused.',
    estimated: 'Estimated space freed',
    estimatedNote:
      'An upper bound. Files sharing storage with a copy elsewhere release less than their reported size.',
    copy: 'Copy plan as JSON',
    copied: 'Copied',
    close: 'Close',
    executeTrash: 'Move to Trash',
    executePermanent: 'Delete permanently',
    executing: 'Working…',
    directory: 'folder',
    file: 'file',
  },
  outcome: {
    title: 'Clean finished',
    removed: 'Removed',
    removedCount: '{{count}} items',
    trashNote:
      'Everything above is in the Trash. Space is freed when you empty it; until then it can be put back.',
    permanentNote: 'Everything above is gone from the disk.',
    reported: 'Reported size',
    freed: 'Space freed',
    freedNote:
      'Space freed is measured on the volume rather than added up from the files, so it is the figure `df` agrees with. It reads lower when a file shared storage with a copy elsewhere, and drifts either way if something else was writing at the same time.',
    failed: 'Could not be removed',
    rejected: 'Refused just before deleting',
    rejectedNote:
      'These passed the earlier check but not the one taken immediately before deleting — something on disk changed in between.',
    nothingRemoved: 'Nothing was removed.',
    batch: 'Record id',
    done: 'Done',
  },
  /** The Full Disk Access banner, shared by every page that can be blocked by it. */
  privacy: {
    dev: 'This is a development build, so it runs as a plain binary: the permission belongs to the terminal that launched it, not to SpaceMaster. Grant it to your terminal, or test this on the built app.',
    openSettings: 'Open System Settings',
  },
  /** Why the OS refused a deletion. */
  failures: {
    permissionDenied: 'macOS refused access. Full Disk Access is probably needed.',
    inaccessible: 'Gone or unreadable by the time we got to it.',
    failed: 'macOS reported an error.',
  },
  ledger: {
    unfinished:
      'A previous clean did not finish normally. {{count}} items had already been removed.',
    dismiss: 'Dismiss',
  },
  /** The audit log. No restore button — see the page for why. */
  history: {
    title: 'History',
    intro:
      'Every clean this app has run, written down as it happened. There is nothing to restore from here: items moved to the Trash come back with Finder’s “Put Back”, and a permanent delete is gone.',
    empty: 'Nothing has been deleted yet.',
    /** Past tense, unlike the plan screen's wording for the same two modes. */
    modeTrash: 'Moved to Trash',
    modePermanent: 'Deleted permanently',
    showPaths: 'Show paths',
    hidePaths: 'Hide paths',
    failedCount: '{{count}} not removed',
    notRemoved: 'not removed',
    interrupted: 'This clean did not finish. Anything after the last path listed was never attempted.',
  },
  /** One entry per named row of the Tier A and Tier B catalogs. */
  catalog: {
    npmCacache: {
      title: 'npm package cache',
      description: 'Downloaded packages. npm refetches whatever it needs.',
    },
    npmLogs: {
      title: 'npm debug logs',
      description: 'Logs written when an npm command failed.',
    },
    homebrewCache: {
      title: 'Homebrew downloads',
      description: 'Archives of already-installed formulae. brew redownloads them.',
    },
    bunInstallCache: {
      title: 'Bun package cache',
      description: 'Downloaded packages. bun refetches whatever it needs.',
    },
    pipCache: {
      title: 'pip package cache',
      description: 'Downloaded wheels and HTTP responses. pip refetches them.',
    },
    yarnCacheLibrary: {
      title: 'Yarn package cache',
      description: 'Downloaded packages. Yarn refetches whatever it needs.',
    },
    yarnCacheDot: {
      title: 'Yarn package cache (legacy path)',
      description: 'The cache location used by older Yarn versions.',
    },
    cocoapodsCache: {
      title: 'CocoaPods cache',
      description: 'Downloaded pods and spec data. pod install rebuilds it.',
    },
    xcodeAppCache: {
      title: 'Xcode application cache',
      description: 'Xcode’s own cache — not build output. Projects still build.',
    },
    appLogs: {
      title: 'Application logs',
      description: 'Diagnostic logs written by installed apps.',
    },
    crashReports: {
      title: 'Crash report history',
      description: 'Past crash reports, kept only for reading after the fact.',
    },
    trash: {
      title: 'Trash',
      description: 'Items you already asked macOS to throw away.',
    },
    cargoRegistry: {
      title: 'Cargo registry',
      description: 'Downloaded crate sources. cargo build refetches and recompiles them.',
    },
    pubCache: {
      title: 'Dart/Flutter package cache',
      description: 'Downloaded packages. pub get refetches them.',
    },
    gradleCaches: {
      title: 'Gradle cache',
      description: 'Downloaded dependencies and build state. The next build refetches them.',
    },
    mavenRepository: {
      title: 'Maven repository',
      description: 'Downloaded artifacts. Maven and Gradle refetch what they need.',
    },
    goModCache: {
      title: 'Go module cache',
      description: 'Downloaded modules. go build refetches them.',
    },
    cocoapodsRepos: {
      title: 'CocoaPods spec repos',
      description: 'Cloned podspec repositories. pod repo update clones them again.',
    },
    swiftpmCache: {
      title: 'Swift Package Manager cache',
      description:
        'Cloned dependency repositories and downloaded artifacts. Resolving packages again refetches them.',
    },
  },
  /** Caches we deliberately do not delete, and the command that clears them. */
  advisories: {
    pnpmStore: {
      title: 'pnpm store',
      description:
        'Hard-linked into the node_modules of every project on this machine. Deleting it frees almost nothing and breaks checked-out projects; the command below drops only the parts nothing references.',
    },
  },
  /** Why the Guard refused a path. Reached only when something is misconfigured. */
  rules: {
    nulByte: 'The path is malformed.',
    notAbsolute: 'The path is not absolute.',
    nonNormalComponent: 'The path contains a relative step such as “..”.',
    tooShallow: 'The path is too close to the top of the disk to be safe to delete.',
    outsideRoots: 'The path is outside every folder this app is allowed to clean.',
    rootNotDeletable: 'This folder is shared with other software; only its contents can go.',
    protected: 'This location is protected and is never deleted.',
    wouldTakeProtected: 'Deleting this would take a protected location with it.',
    systemBundle: 'This belongs to macOS itself.',
    ownAppData: 'This is SpaceMaster’s own data.',
    missing: 'The path no longer exists.',
    symlink: 'This is a shortcut to somewhere else, so it is left alone.',
    notFileOrDir: 'This is neither a file nor a folder.',
    otherVolume: 'This lives on a different disk.',
    pathAliased: 'The path resolves somewhere other than where it points.',
    appRunning: 'The app that owns this is running.',
    permanentNotAllowed: 'Permanent deletion is not allowed for this location.',
    overlapping: 'Another selected item contains this one.',
  },
  /** Why an entry was skipped while measuring. Not failures — just not counted. */
  issues: {
    permissionDenied: 'Not readable',
    symlinkSkipped: 'Shortcut, not followed',
    otherVolumeSkipped: 'On another disk',
    readError: 'Could not be read',
    missing: 'Disappeared during the scan',
  },
  /** Shown by the error boundary, i.e. when the interface itself failed to render. */
  crash: {
    title: 'The interface stopped',
    body: 'Something in the app failed to draw. Nothing was deleted by this — deletions only ever happen when you press the button, and the History page lists every one that did.',
    reload: 'Reload',
  },
  errors: {
    invalidPath: 'Invalid path: {{detail}}',
    io: 'A disk operation failed: {{detail}}',
    scan: 'The scan could not start: {{detail}}',
    staleScan: 'These results are out of date. Scan again.',
    stalePlan: 'This plan is no longer valid. Review the selection again.',
    unknown: 'Unexpected error: {{detail}}',
  },
} as const

export type Translation = typeof en

/** Ids of the named catalog rows, derived from the wording so the two cannot drift. */
export type CatalogId = keyof Translation['catalog']
export type AdvisoryId = keyof Translation['advisories']
export type NoteId = keyof Translation['notes']

/**
 * The shape every non-base locale must satisfy: same namespaces, same keys, plain
 * (non-literal) strings as values. `catalog` nests one level deeper than the rest.
 */
export type LocaleResource = {
  [Namespace in keyof Translation]: {
    [Key in keyof Translation[Namespace]]: Translation[Namespace][Key] extends string
      ? string
      : { [Sub in keyof Translation[Namespace][Key]]: string }
  }
}
