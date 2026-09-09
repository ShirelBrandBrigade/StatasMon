// Dev-only entry: `pnpm web` then open http://localhost:1420/src/gallery/index.html
// It is not part of the production build (only the root index.html is).
import React from "react";
import ReactDOM from "react-dom/client";
import "../styles.css";
import { Gallery } from "./Gallery";

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <Gallery />
  </React.StrictMode>,
);
