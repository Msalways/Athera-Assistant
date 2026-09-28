import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  onReset?: () => void;
}

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Athera UI failed to render", error, info.componentStack);
  }

  private reset = () => {
    this.setState({ error: null });
    this.props.onReset?.();
  };

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="fatal-error" role="alert">
        <h1>Athera hit an unexpected error</h1>
        <p>
          Your saved conversations and provider settings are untouched. Reload
          the screen to continue.
        </p>
        <button className="primary" type="button" onClick={this.reset}>
          Reload
        </button>
      </div>
    );
  }
}
