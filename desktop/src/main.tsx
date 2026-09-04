import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";

// Release builds ship without devtools, so fatal errors are mirrored into
// the window title where they stay visible for diagnosis.
const surface = (msg: string) => {
  document.title = `ERROR: ${msg}`.slice(0, 200);
};
window.addEventListener("error", (e) => surface(e.message));
window.addEventListener("unhandledrejection", (e) => surface(String(e.reason)));

class Boundary extends React.Component<{ children: React.ReactNode }, { err: string }> {
  state = { err: "" };
  static getDerivedStateFromError(e: unknown) {
    return { err: e instanceof Error ? e.message : String(e) };
  }
  render() {
    if (this.state.err) {
      surface(this.state.err);
      return <div style={{ padding: 20, fontFamily: "monospace", color: "#b91c1c" }}>{this.state.err}</div>;
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Boundary>
      <App />
    </Boundary>
  </React.StrictMode>,
);
