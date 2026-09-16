import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import RemoteWindow from "./RemoteWindow";
import FileTransfer from "./FileTransfer";
import "./styles/app.css";

const view = new URLSearchParams(window.location.search).get("view");

function Root() {
  if (view === "remote") return <RemoteWindow />;
  if (view === "files") return <FileTransfer />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
