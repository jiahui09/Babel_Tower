import { Component, type ErrorInfo, type PropsWithChildren, type ReactNode } from "react";
import { withTranslation, type WithTranslation } from "react-i18next";

import { Button } from "../components/ui/button";

interface State {
  error: Error | null;
}

class AppErrorBoundaryBase extends Component<PropsWithChildren<WithTranslation>, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Babel Tower UI error", error, info.componentStack);
  }

  render(): ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <main className="grid h-full place-items-center bg-[var(--surface)] p-8">
        <section className="w-full max-w-[560px] border border-[var(--border)] bg-[var(--surface-raised)] p-6">
          <h1 className="m-0 text-base font-semibold">{this.props.t("title", { ns: "errors" })}</h1>
          <p className="mt-3 text-sm text-[var(--text-secondary)]">
            {this.props.t("unexpected", { ns: "errors" })}
          </p>
          <pre className="max-h-40 overflow-auto bg-[var(--surface-inset)] p-3 text-xs text-[var(--danger)]">
            {this.state.error.message}
          </pre>
          <Button className="mt-4" onClick={() => window.location.reload()}>
            {this.props.t("retry", { ns: "common" })}
          </Button>
        </section>
      </main>
    );
  }
}

export const AppErrorBoundary = withTranslation(["errors", "common"])(AppErrorBoundaryBase);
