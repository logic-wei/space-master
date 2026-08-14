/**
 * The sections of the app, in render and reading order: the one-click page first, then
 * the professional ones from broadest to narrowest, and the audit log last.
 *
 * Its own module rather than a constant beside the nav, because `App` maps it to page
 * components and a file that exports both a component and a value loses fast refresh.
 */
export const TABS = ['quick', 'dev', 'xcode', 'simulators', 'orphans', 'history'] as const

export type Tab = (typeof TABS)[number]
