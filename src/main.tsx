import { t } from "./i18n";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "@fontsource-variable/inter";
import "./styles/tokens.css";
import "./styles/app.css";
if (new URLSearchParams(location.search).get("window") === "shelf")
  document.documentElement.dataset.shelf = "true";
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: string }
> {
  state = { error: "" };
  static getDerivedStateFromError(error: unknown) {
    return { error: String(error) };
  }
  render() {
    return this.state.error ? (
      <div className="app-start">
        <h2>{t("界面遇到问题")}</h2>
        <p>{this.state.error}</p>
        <button className="button" onClick={() => location.reload()}>
          {t("重新载入")}
        </button>
      </div>
    ) : (
      this.props.children
    );
  }
}
ReactDOM.createRoot(document.getElementById("root")!).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);
