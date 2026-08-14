import { Component, type ErrorInfo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

/**
 * The last resort for a render that throws.
 *
 * Without one, React unmounts the whole tree and leaves an empty window — which in a
 * native-looking app reads as "the app is broken" with nothing to act on. This is a
 * class component because that is the only form React offers for catching a render
 * error; there is no hook equivalent.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, { message: string | null }> {
  state = { message: null as string | null }

  static getDerivedStateFromError(error: unknown) {
    return { message: error instanceof Error ? error.message : String(error) }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // The devtools console in a dev build, and nowhere in a bundled one. The message
    // shown on screen is what a user can actually report back.
    console.error(error, info.componentStack)
  }

  render() {
    if (this.state.message === null) return this.props.children
    return <Crashed message={this.state.message} />
  }
}

/**
 * Split out so the message can be translated: hooks are unavailable inside the class,
 * and this is the one screen where hardcoded English would be the easy way out.
 */
function Crashed({ message }: { message: string }) {
  const { t } = useTranslation()
  return (
    <main className="crash">
      <section className="card">
        <div className="card__head">
          <h2 className="card__title">{t('crash.title')}</h2>
        </div>
        <p className="card__note">{t('crash.body')}</p>
        <p className="row__path num">{message}</p>
        {/* Reload rather than "try again": the state that produced the throw is still
            in memory, and re-rendering it would throw again. Nothing is lost — every
            page starts from a scan the user asks for. */}
        <div className="foot">
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => window.location.reload()}
          >
            {t('crash.reload')}
          </button>
        </div>
      </section>
    </main>
  )
}
