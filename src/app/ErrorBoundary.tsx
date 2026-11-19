import { Component, type ReactNode } from 'react';

/** Without this, one render error removes the whole UI and leaves an empty window. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div role="alert" className="grid h-screen place-items-center bg-canvas p-8 text-fg">
        <div className="max-w-md space-y-3">
          <h1 className="text-xl font-semibold">Something went wrong</h1>
          <p className="text-fg-muted">
            Orbit hit an unexpected error. Your services keep running. Reload to continue.
          </p>
          <pre className="overflow-auto rounded-card bg-raised p-3 text-xs">{error.message}</pre>
          <button
            type="button"
            className="rounded-full bg-accent px-4 py-2 text-accent-fg"
            onClick={() => location.reload()}
          >
            Reload
          </button>
        </div>
      </div>
    );
  }
}
