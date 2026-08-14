import { useEffect, useRef, useState } from 'react'

/**
 * Copies a string and says so for a moment.
 *
 * The reset is why this is a component rather than two lines at each call site: a latch
 * that never clears leaves "Copied" showing indefinitely, so a later copy of a
 * *different* value looks like the button did nothing.
 */
export function CopyButton({
  value,
  label,
  copiedLabel,
}: {
  value: string
  label: string
  copiedLabel: string
}) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<number | null>(null)

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current)
    },
    [],
  )

  const copy = () => {
    void navigator.clipboard.writeText(value).then(
      () => {
        setCopied(true)
        if (timer.current !== null) window.clearTimeout(timer.current)
        timer.current = window.setTimeout(() => setCopied(false), 1800)
      },
      // A rejected write put nothing on the clipboard, so the label must not claim it did.
      () => {},
    )
  }

  return (
    <button type="button" className="btn btn--quiet" onClick={copy}>
      {copied ? copiedLabel : label}
    </button>
  )
}
