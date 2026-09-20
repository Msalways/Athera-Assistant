import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import SmsExperiment from "./SmsExperiment";
import "./styles.css";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {import.meta.env.MODE === "sms" ? <SmsExperiment /> : <App />}
  </React.StrictMode>,
);
