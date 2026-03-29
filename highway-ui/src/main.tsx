import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import { initTransport } from "./transport/index.js";
import "./index.css";

// Initialize gRPC-Web transport — points to the runtime highway service.
// In development, Vite proxies /wacp.v1.HighwayService to localhost:9091.
initTransport(window.location.origin);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
